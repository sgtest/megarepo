package jobutil

import (
	"strconv"
	"strings"

	"github.com/grafana/regexp"
	"github.com/inconshreveable/log15"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/commit"
	"github.com/sourcegraph/sourcegraph/internal/search/filter"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/limits"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	searchrepos "github.com/sourcegraph/sourcegraph/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/run"
	"github.com/sourcegraph/sourcegraph/internal/search/searchcontexts"
	"github.com/sourcegraph/sourcegraph/internal/search/searcher"
	"github.com/sourcegraph/sourcegraph/internal/search/structural"
	"github.com/sourcegraph/sourcegraph/internal/search/symbol"
	zoektutil "github.com/sourcegraph/sourcegraph/internal/search/zoekt"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

// ToSearchJob converts a query parse tree to the _internal_ representation
// needed to run a search routine. To understand why this conversion matters, think
// about the fact that the query parse tree doesn't know anything about our
// backends or architecture. It doesn't decide certain defaults, like whether we
// should return multiple result types (pattern matches content, or a file name,
// or a repo name). If we want to optimize a Sourcegraph query parse tree for a
// particular backend (e.g., skip repository resolution and just run a Zoekt
// query on all indexed repositories) then we need to convert our tree to
// Zoekt's internal inputs and representation. These concerns are all handled by
// toSearchJob.
func ToSearchJob(searchInputs *run.SearchInputs, q query.Q) (job.Job, error) {
	maxResults := q.MaxResults(searchInputs.DefaultLimit())

	b, err := query.ToBasicQuery(q)
	if err != nil {
		return nil, err
	}
	types, _ := q.StringValues(query.FieldType)
	resultTypes := search.ComputeResultTypes(types, b, searchInputs.PatternType)
	patternInfo := search.ToTextPatternInfo(b, resultTypes, searchInputs.Protocol)

	// searcher to use full deadline if timeout: set or we are streaming.
	useFullDeadline := q.Timeout() != nil || q.Count() != nil || searchInputs.Protocol == search.Streaming

	fileMatchLimit := int32(computeFileMatchLimit(b, searchInputs.Protocol))
	selector, _ := filter.SelectPathFromString(b.FindValue(query.FieldSelect)) // Invariant: select is validated

	features := toFeatures(searchInputs.Features)
	repoOptions := toRepoOptions(q, searchInputs.UserSettings)

	builder := &jobBuilder{
		query:          b,
		resultTypes:    resultTypes,
		repoOptions:    repoOptions,
		features:       &features,
		fileMatchLimit: fileMatchLimit,
		selector:       selector,
	}

	repoUniverseSearch, skipRepoSubsetSearch, runZoektOverRepos := jobMode(b, resultTypes, searchInputs.PatternType, searchInputs.OnSourcegraphDotCom)

	var requiredJobs, optionalJobs []job.Job
	addJob := func(required bool, job job.Job) {
		if required {
			requiredJobs = append(requiredJobs, job)
		} else {
			optionalJobs = append(optionalJobs, job)
		}
	}

	{
		// This code block creates search jobs under specific
		// conditions, and depending on generic process of `args` above.
		// It which specializes search logic in doResults. In time, all
		// of the above logic should be used to create search jobs
		// across all of Sourcegraph.

		// Create Text Search Jobs
		if resultTypes.Has(result.TypeFile | result.TypePath) {
			// Create Global Text Search jobs.
			if repoUniverseSearch {
				job, err := builder.newZoektGlobalSearch(search.TextRequest)
				if err != nil {
					return nil, err
				}
				addJob(true, job)
			}

			// Create Text Search jobs over repo set.
			if !skipRepoSubsetSearch {
				var textSearchJobs []job.Job
				if runZoektOverRepos {
					job, err := builder.newZoektSearch(search.TextRequest)
					if err != nil {
						return nil, err
					}
					textSearchJobs = append(textSearchJobs, job)
				}

				textSearchJobs = append(textSearchJobs, &searcher.Searcher{
					PatternInfo:     patternInfo,
					Indexed:         false,
					UseFullDeadline: useFullDeadline,
				})

				addJob(true, &repoPagerJob{
					child:            NewParallelJob(textSearchJobs...),
					repoOptions:      repoOptions,
					useIndex:         b.Index(),
					containsRefGlobs: query.ContainsRefGlobs(q),
				})
			}
		}

		// Create Symbol Search Jobs
		if resultTypes.Has(result.TypeSymbol) {
			// Create Global Symbol Search jobs.
			if repoUniverseSearch {
				job, err := builder.newZoektGlobalSearch(search.SymbolRequest)
				if err != nil {
					return nil, err
				}
				addJob(true, job)
			}

			// Create Symbol Search jobs over repo set.
			if !skipRepoSubsetSearch {
				var symbolSearchJobs []job.Job

				if runZoektOverRepos {
					job, err := builder.newZoektSearch(search.SymbolRequest)
					if err != nil {
						return nil, err
					}
					symbolSearchJobs = append(symbolSearchJobs, job)
				}

				symbolSearchJobs = append(symbolSearchJobs, &searcher.SymbolSearcher{
					PatternInfo: patternInfo,
					Limit:       maxResults,
				})

				required := useFullDeadline || resultTypes.Without(result.TypeSymbol) == 0
				addJob(required, &repoPagerJob{
					child:            NewParallelJob(symbolSearchJobs...),
					repoOptions:      repoOptions,
					useIndex:         q.Index(),
					containsRefGlobs: query.ContainsRefGlobs(q),
				})
			}
		}

		if resultTypes.Has(result.TypeCommit) || resultTypes.Has(result.TypeDiff) {
			diff := resultTypes.Has(result.TypeDiff)
			var required bool
			if useFullDeadline {
				required = true
			} else if diff {
				required = resultTypes.Without(result.TypeDiff) == 0
			} else {
				required = resultTypes.Without(result.TypeCommit) == 0
			}
			addJob(required, &commit.CommitSearch{
				Query:                commit.QueryToGitQuery(q, diff),
				RepoOpts:             repoOptions,
				Diff:                 diff,
				HasTimeFilter:        b.Exists("after") || b.Exists("before"),
				Limit:                int(fileMatchLimit),
				IncludeModifiedFiles: authz.SubRepoEnabled(authz.DefaultSubRepoPermsChecker),
			})
		}

		if resultTypes.Has(result.TypeStructural) {
			typ := search.TextRequest
			zoektQuery, err := search.QueryToZoektQuery(b, resultTypes, &features, typ)
			if err != nil {
				return nil, err
			}
			zoektArgs := &search.ZoektParameters{
				Query:          zoektQuery,
				Typ:            typ,
				FileMatchLimit: fileMatchLimit,
				Select:         selector,
			}

			searcherArgs := &search.SearcherParameters{
				PatternInfo:     patternInfo,
				UseFullDeadline: useFullDeadline,
			}

			addJob(true, &structural.StructuralSearch{
				ZoektArgs:        zoektArgs,
				SearcherArgs:     searcherArgs,
				UseIndex:         b.Index(),
				ContainsRefGlobs: query.ContainsRefGlobs(q),
				RepoOpts:         repoOptions,
			})
		}

		if resultTypes.Has(result.TypeRepo) {
			valid := func() bool {
				fieldAllowlist := map[string]struct{}{
					query.FieldRepo:               {},
					query.FieldContext:            {},
					query.FieldType:               {},
					query.FieldDefault:            {},
					query.FieldIndex:              {},
					query.FieldCount:              {},
					query.FieldTimeout:            {},
					query.FieldFork:               {},
					query.FieldArchived:           {},
					query.FieldVisibility:         {},
					query.FieldCase:               {},
					query.FieldRepoHasFile:        {},
					query.FieldRepoHasCommitAfter: {},
					query.FieldPatternType:        {},
					query.FieldSelect:             {},
				}

				// Don't run a repo search if the search contains fields that aren't on the allowlist.
				exists := true
				query.VisitParameter(q, func(field, _ string, _ bool, _ query.Annotation) {
					if _, ok := fieldAllowlist[field]; !ok {
						exists = false
					}
				})
				return exists
			}

			// returns an updated RepoOptions if the pattern part of a query can be used to
			// search repos. A problematic case we check for is when the pattern contains `@`,
			// which may confuse downstream logic to interpret it as part of `repo@rev` syntax.
			addPatternAsRepoFilter := func(pattern string, opts search.RepoOptions) (search.RepoOptions, bool) {
				if pattern == "" {
					return opts, true
				}

				opts.RepoFilters = append(make([]string, 0, len(opts.RepoFilters)), opts.RepoFilters...)
				opts.CaseSensitiveRepoFilters = q.IsCaseSensitive()

				patternPrefix := strings.SplitN(pattern, "@", 2)
				if len(patternPrefix) == 1 {
					// No "@" in pattern? We're good.
					opts.RepoFilters = append(opts.RepoFilters, pattern)
					return opts, true
				}

				if patternPrefix[0] != "" {
					// Extend the repo search using the pattern value, but
					// since the pattern contains @, only search the part
					// prefixed by the first @. This because downstream
					// logic will get confused by the presence of @ and try
					// to resolve repo revisions. See #27816.
					if _, err := regexp.Compile(patternPrefix[0]); err != nil {
						// Prefix is not valid regexp, so just reject it. This can happen for patterns where we've automatically added `(...).*?(...)`
						// such as `foo @bar` which becomes `(foo).*?(@bar)`, which when stripped becomes `(foo).*?(` which is unbalanced and invalid.
						// Why is this a mess? Because validation for everything, including repo values, should be done up front so far possible, not downtsream
						// after possible modifications. By the time we reach this code, the pattern should already have been considered valid to continue with
						// a search. But fixing the order of concerns for repo code is not something @rvantonder is doing today.
						return search.RepoOptions{}, false
					}
					opts.RepoFilters = append(opts.RepoFilters, patternPrefix[0])
					return opts, true
				}

				// This pattern starts with @, of the form "@thing". We can't
				// consistently handle search repos of this form, because
				// downstream logic will attempt to interpret "thing" as a repo
				// revision, may fail, and cause us to raise an alert for any
				// non `type:repo` search. Better to not attempt a repo search.
				return search.RepoOptions{}, false
			}

			if valid() {
				if repoOptions, ok := addPatternAsRepoFilter(b.PatternString(), repoOptions); ok {
					var mode search.GlobalSearchMode
					if repoUniverseSearch {
						mode = search.ZoektGlobalSearch
					}
					if skipRepoSubsetSearch {
						mode = search.SkipUnindexed
					}
					addJob(true, &run.RepoSearch{
						RepoOptions:                  repoOptions,
						Features:                     features,
						FilePatternsReposMustInclude: patternInfo.FilePatternsReposMustInclude,
						FilePatternsReposMustExclude: patternInfo.FilePatternsReposMustExclude,
						Mode:                         mode,
					})
				}
			}
		}
	}

	addJob(true, &searchrepos.ComputeExcludedRepos{
		Options: repoOptions,
	})

	job := NewPriorityJob(
		NewParallelJob(requiredJobs...),
		NewParallelJob(optionalJobs...),
	)

	checker := authz.DefaultSubRepoPermsChecker
	if authz.SubRepoEnabled(checker) {
		job = NewFilterJob(job)
	}

	return job, nil
}

func toRepoOptions(q query.Q, userSettings *schema.Settings) search.RepoOptions {
	repoFilters, minusRepoFilters := q.Repositories()

	var settingForks, settingArchived bool
	if v := userSettings.SearchIncludeForks; v != nil {
		settingForks = *v
	}
	if v := userSettings.SearchIncludeArchived; v != nil {
		settingArchived = *v
	}

	fork := query.No
	if searchrepos.ExactlyOneRepo(repoFilters) || settingForks {
		// fork defaults to No unless either of:
		// (1) exactly one repo is being searched, or
		// (2) user/org/global setting includes forks
		fork = query.Yes
	}
	if setFork := q.Fork(); setFork != nil {
		fork = *setFork
	}

	archived := query.No
	if searchrepos.ExactlyOneRepo(repoFilters) || settingArchived {
		// archived defaults to No unless either of:
		// (1) exactly one repo is being searched, or
		// (2) user/org/global setting includes archives in all searches
		archived = query.Yes
	}
	if setArchived := q.Archived(); setArchived != nil {
		archived = *setArchived
	}

	visibilityStr, _ := q.StringValue(query.FieldVisibility)
	visibility := query.ParseVisibility(visibilityStr)

	commitAfter, _ := q.StringValue(query.FieldRepoHasCommitAfter)
	searchContextSpec, _ := q.StringValue(query.FieldContext)

	return search.RepoOptions{
		RepoFilters:       repoFilters,
		MinusRepoFilters:  minusRepoFilters,
		Dependencies:      q.Dependencies(),
		SearchContextSpec: searchContextSpec,
		ForkSet:           q.Fork() != nil,
		OnlyForks:         fork == query.Only,
		NoForks:           fork == query.No,
		ArchivedSet:       q.Archived() != nil,
		OnlyArchived:      archived == query.Only,
		NoArchived:        archived == query.No,
		Visibility:        visibility,
		CommitAfter:       commitAfter,
	}
}

// jobBuilder represents computed static values that are backend agnostic: we
// generally need to compute these values before we're able to create (or build)
// multiple specific jobs. If you want to add new fields or state to run a
// search, ask yourself: is this value specific to a backend like Zoekt,
// searcher, or gitserver, or a new backend? If yes, then that new field does
// not belong in this builder type, and your new field should probably be
// computed either using values in this builder, or obtained from the outside
// world where you construct your specific search job.
//
// If you _may_ need the value available to start a search across differnt
// backends, then this builder type _may_ be the right place for it to live.
// If in doubt, ask the search team.
type jobBuilder struct {
	query          query.Basic
	resultTypes    result.Types
	repoOptions    search.RepoOptions
	features       *search.Features
	fileMatchLimit int32
	selector       filter.SelectPath
}

func (b *jobBuilder) newZoektGlobalSearch(typ search.IndexedRequestType) (job.Job, error) {
	zoektQuery, err := search.QueryToZoektQuery(b.query, b.resultTypes, b.features, typ)
	if err != nil {
		return nil, err
	}

	defaultScope, err := zoektutil.DefaultGlobalQueryScope(b.repoOptions)
	if err != nil {
		return nil, err
	}

	includePrivate := b.repoOptions.Visibility == query.Private || b.repoOptions.Visibility == query.Any
	globalZoektQuery := zoektutil.NewGlobalZoektQuery(zoektQuery, defaultScope, includePrivate)

	zoektArgs := &search.ZoektParameters{
		// TODO(rvantonder): the Query value is set when the global zoekt query is
		// enriched with private repository data in the search job's Run method, and
		// is therefore set to `nil` below.
		// Ideally, The ZoektParameters type should not expose this field for Universe text
		// searches at all, and will be removed once jobs are fully migrated.
		Query:          nil,
		Typ:            typ,
		FileMatchLimit: b.fileMatchLimit,
		Select:         b.selector,
	}

	switch typ {
	case search.SymbolRequest:
		return &symbol.RepoUniverseSymbolSearch{
			GlobalZoektQuery: globalZoektQuery,
			ZoektArgs:        zoektArgs,
			RepoOptions:      b.repoOptions,
		}, nil
	case search.TextRequest:
		return &zoektutil.GlobalSearch{
			GlobalZoektQuery: globalZoektQuery,
			ZoektArgs:        zoektArgs,
			RepoOptions:      b.repoOptions,
		}, nil
	}
	return nil, errors.Errorf("attempt to create unrecognized zoekt global search with value %v", typ)
}

func (b *jobBuilder) newZoektSearch(typ search.IndexedRequestType) (job.Job, error) {
	zoektQuery, err := search.QueryToZoektQuery(b.query, b.resultTypes, b.features, typ)
	if err != nil {
		return nil, err
	}

	switch typ {
	case search.SymbolRequest:
		return &zoektutil.ZoektSymbolSearch{
			Query:          zoektQuery,
			FileMatchLimit: b.fileMatchLimit,
			Select:         b.selector,
		}, nil
	case search.TextRequest:
		return &zoektutil.ZoektRepoSubsetSearch{
			Query:          zoektQuery,
			Typ:            typ,
			FileMatchLimit: b.fileMatchLimit,
			Select:         b.selector,
		}, nil
	}
	return nil, errors.Errorf("attempt to create unrecognized zoekt search with value %v", typ)
}

func jobMode(b query.Basic, resultTypes result.Types, st query.SearchType, onSourcegraphDotCom bool) (repoUniverseSearch, skipRepoSubsetSearch, runZoektOverRepos bool) {
	isGlobalSearch := func() bool {
		if st == query.SearchTypeStructural {
			return false
		}

		return query.ForAll(b.ToParseTree(), func(node query.Node) bool {
			n, ok := node.(query.Parameter)
			if !ok {
				return true
			}
			switch n.Field {
			case query.FieldContext:
				return searchcontexts.IsGlobalSearchContextSpec(n.Value)
			case query.FieldRepo:
				// We allow -repo: in global search.
				return n.Negated
			case query.FieldRepoHasFile:
				return false
			default:
				return true
			}
		})
	}

	hasGlobalSearchResultType := resultTypes.Has(result.TypeFile | result.TypePath | result.TypeSymbol)
	isIndexedSearch := b.Index() != query.No
	noPattern := b.IsEmptyPattern()
	noFile := !b.Exists(query.FieldFile)
	noLang := !b.Exists(query.FieldLang)
	isEmpty := noPattern && noFile && noLang

	repoUniverseSearch = isGlobalSearch() && isIndexedSearch && hasGlobalSearchResultType && !isEmpty
	// skipRepoSubsetSearch is a value that controls whether to
	// run unindexed search in a specific scenario of queries that
	// contain no repo-affecting filters (global mode). When on
	// sourcegraph.com, we resolve only a subset of all indexed
	// repos to search. This control flow implies len(searcherRepos)
	// is always 0, meaning that we should not create jobs to run
	// unindexed searcher.
	skipRepoSubsetSearch = isEmpty || (repoUniverseSearch && onSourcegraphDotCom)

	// runZoektOverRepos controls whether we run Zoekt over a set of
	// resolved repositories. Because Zoekt can run natively run over all
	// repositories (AKA global search), we can sometimes skip searching
	// over resolved repos.
	//
	// The decision to run over a set of repos is as follows:
	// (1) When we don't run global search, run Zoekt over repositories (we have to, otherwise
	// we'd be skipping indexed search entirely).
	// (2) If on Sourcegraph.com, resolve repos unconditionally (we run both global search
	// and search over resolved repos, and return results from either job).
	runZoektOverRepos = !repoUniverseSearch || onSourcegraphDotCom

	return repoUniverseSearch, skipRepoSubsetSearch, runZoektOverRepos
}

func toFeatures(flags featureflag.FlagSet) search.Features {
	if flags == nil {
		flags = featureflag.FlagSet{}
		metricFeatureFlagUnavailable.Inc()
		log15.Warn("search feature flags are not available")
	}

	return search.Features{
		ContentBasedLangFilters: flags.GetBoolOr("search-content-based-lang-detection", false),
	}
}

// toAndJob creates a new job from a basic query whose pattern is an And operator at the root.
func toAndJob(inputs *run.SearchInputs, q query.Basic) (job.Job, error) {
	// Invariant: this function is only reachable from callers that
	// guarantee a root node with one or more queryOperands.
	queryOperands := q.Pattern.(query.Operator).Operands

	// Limit the number of results from each child to avoid a huge amount of memory bloat.
	// With streaming, we should re-evaluate this number.
	//
	// NOTE: It may be possible to page over repos so that each intersection is only over
	// a small set of repos, limiting massive number of results that would need to be
	// kept in memory otherwise.
	maxTryCount := 40000

	operands := make([]job.Job, 0, len(queryOperands))
	for _, queryOperand := range queryOperands {
		operand, err := toPatternExpressionJob(inputs, q.MapPattern(queryOperand))
		if err != nil {
			return nil, err
		}
		operands = append(operands, NewLimitJob(maxTryCount, operand))
	}

	return NewAndJob(operands...), nil
}

// toOrJob creates a new job from a basic query whose pattern is an Or operator at the top level
func toOrJob(inputs *run.SearchInputs, q query.Basic) (job.Job, error) {
	// Invariant: this function is only reachable from callers that
	// guarantee a root node with one or more queryOperands.
	queryOperands := q.Pattern.(query.Operator).Operands

	operands := make([]job.Job, 0, len(queryOperands))
	for _, term := range queryOperands {
		operand, err := toPatternExpressionJob(inputs, q.MapPattern(term))
		if err != nil {
			return nil, err
		}
		operands = append(operands, operand)
	}
	return NewOrJob(operands...), nil
}

func toPatternExpressionJob(inputs *run.SearchInputs, q query.Basic) (job.Job, error) {
	switch term := q.Pattern.(type) {
	case query.Operator:
		if len(term.Operands) == 0 {
			return NewNoopJob(), nil
		}

		switch term.Kind {
		case query.And:
			return toAndJob(inputs, q)
		case query.Or:
			return toOrJob(inputs, q)
		case query.Concat:
			return ToSearchJob(inputs, q.ToParseTree())
		}
	case query.Pattern:
		return ToSearchJob(inputs, q.ToParseTree())
	case query.Parameter:
		// evaluatePatternExpression does not process Parameter nodes.
		return NewNoopJob(), nil
	}
	// Unreachable.
	return nil, errors.Errorf("unrecognized type %T in evaluatePatternExpression", q.Pattern)
}

func ToEvaluateJob(inputs *run.SearchInputs, q query.Basic) (job.Job, error) {
	maxResults := q.ToParseTree().MaxResults(inputs.DefaultLimit())
	timeout := search.TimeoutDuration(q)

	var (
		job job.Job
		err error
	)
	if q.Pattern == nil {
		job, err = ToSearchJob(inputs, query.ToNodes(q.Parameters))
	} else {
		job, err = toPatternExpressionJob(inputs, q)
		if err != nil {
			return nil, err
		}
		if _, ok := q.Pattern.(query.Pattern); !ok {
			// This pattern is not an atomic Pattern, but an
			// expression. Optimize the expression for backends.
			job, err = optimizeJobs(job, inputs, q.ToParseTree())
		}
	}
	if err != nil {
		return nil, err
	}

	if v, _ := q.ToParseTree().StringValue(query.FieldSelect); v != "" {
		sp, _ := filter.SelectPathFromString(v) // Invariant: select already validated
		job = NewSelectJob(sp, job)
	}

	return NewTimeoutJob(timeout, NewLimitJob(maxResults, job)), nil
}

// optimizeJobs optimizes a baseJob query. It does this by calling ToSearchJob
// with a query that has a more expressive shape (and/or/not expressions) and
// converts them directly to native queries for a backed. Currently that backend
// is Zoekt. It removes unoptimized Zoekt jobs from the baseJob and repalces it
// with the optimized ones.
func optimizeJobs(baseJob job.Job, inputs *run.SearchInputs, q query.Q) (job.Job, error) {
	candidateOptimizedJobs, err := ToSearchJob(inputs, q)
	if err != nil {
		return nil, err
	}

	var optimizedJobs []job.Job
	collector := Mapper{
		MapJob: func(currentJob job.Job) job.Job {
			switch currentJob.(type) {
			case
				*zoektutil.GlobalSearch,
				*symbol.RepoUniverseSymbolSearch,
				*zoektutil.ZoektRepoSubsetSearch,
				*zoektutil.ZoektSymbolSearch:
				optimizedJobs = append(optimizedJobs, currentJob)
				return currentJob
			default:
				return currentJob
			}
		},
	}

	collector.Map(candidateOptimizedJobs)

	// We've created optimized jobs. Now let's remove any unoptimized ones
	// in the job expression tree. We trim off any jobs corresponding to
	// optimized ones (if we created an optimized global zoekt jobs, we
	// delete all global zoekt jobs created by the default strategy).

	exists := func(name string) bool {
		for _, j := range optimizedJobs {
			if name == j.Name() {
				return true
			}
		}
		return false
	}

	trimmer := Mapper{
		MapJob: func(currentJob job.Job) job.Job {
			switch currentJob.(type) {
			case *zoektutil.GlobalSearch:
				if exists("ZoektGlobalSearch") {
					return &noopJob{}
				}
				return currentJob

			case *zoektutil.ZoektRepoSubsetSearch:
				if exists("ZoektRepoSubset") {
					return &noopJob{}
				}
				return currentJob

			case *zoektutil.ZoektSymbolSearch:
				if exists("ZoektSymbolSearch") {
					return &noopJob{}
				}
				return currentJob

			case *symbol.RepoUniverseSymbolSearch:
				if exists("RepoUniverseSymbolSearch") {
					return &noopJob{}
				}
				return currentJob

			default:
				return currentJob
			}
		},
	}

	trimmedJob := trimmer.Map(baseJob)

	// wrap the optimized jobs that require repo pager
	for i, job := range optimizedJobs {
		switch job.(type) {
		case
			*zoektutil.ZoektRepoSubsetSearch,
			*zoektutil.ZoektSymbolSearch:
			optimizedJobs[i] = &repoPagerJob{
				child:            job,
				repoOptions:      toRepoOptions(q, inputs.UserSettings),
				useIndex:         q.Index(),
				containsRefGlobs: query.ContainsRefGlobs(q),
			}
		}
	}

	optimizedJob := NewParallelJob(optimizedJobs...)

	// wrap optimized jobs in the permissions checker
	checker := authz.DefaultSubRepoPermsChecker
	if authz.SubRepoEnabled(checker) {
		optimizedJob = NewFilterJob(optimizedJob)
	}

	return NewParallelJob(optimizedJob, trimmedJob), nil
}

// FromExpandedPlan takes a query plan that has had all predicates expanded,
// and converts it to a job.
func FromExpandedPlan(inputs *run.SearchInputs, plan query.Plan) (job.Job, error) {
	children := make([]job.Job, 0, len(plan))
	for _, q := range plan {
		child, err := ToEvaluateJob(inputs, q)
		if err != nil {
			return nil, err
		}
		children = append(children, child)
	}
	return NewAlertJob(inputs, NewOrJob(children...)), nil
}

var metricFeatureFlagUnavailable = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_search_featureflag_unavailable",
	Help: "temporary counter to check if we have feature flag available in practice.",
})

func computeFileMatchLimit(q query.Basic, p search.Protocol) int {
	if count := q.GetCount(); count != "" {
		v, _ := strconv.Atoi(count) // Invariant: count is validated.
		return v
	}

	if q.IsStructural() {
		return limits.DefaultMaxSearchResults
	}

	switch p {
	case search.Batch:
		return limits.DefaultMaxSearchResults
	case search.Streaming:
		return limits.DefaultMaxSearchResultsStreaming
	}
	panic("unreachable")
}
