package graphqlbackend

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/neelance/parallel"
	"github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"golang.org/x/sync/errgroup"
	"golang.org/x/sync/semaphore"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	searchlogs "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/search/logs"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/deviceid"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/honey"
	searchhoney "github.com/sourcegraph/sourcegraph/internal/honey/search"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/alert"
	"github.com/sourcegraph/sourcegraph/internal/search/commit"
	"github.com/sourcegraph/sourcegraph/internal/search/filter"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	searchrepos "github.com/sourcegraph/sourcegraph/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/run"
	"github.com/sourcegraph/sourcegraph/internal/search/searchcontexts"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/search/structural"
	"github.com/sourcegraph/sourcegraph/internal/search/symbol"
	"github.com/sourcegraph/sourcegraph/internal/search/textsearch"
	zoektutil "github.com/sourcegraph/sourcegraph/internal/search/zoekt"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/usagestats"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func (c *SearchResultsResolver) LimitHit() bool {
	return c.Stats.IsLimitHit || (c.limit > 0 && len(c.Matches) > c.limit)
}

func (c *SearchResultsResolver) matchesRepoIDs() map[api.RepoID]struct{} {
	m := map[api.RepoID]struct{}{}
	for _, id := range c.Matches {
		m[id.RepoName().ID] = struct{}{}
	}
	return m
}

func (c *SearchResultsResolver) Repositories(ctx context.Context) ([]*RepositoryResolver, error) {
	// c.Stats.Repos does not necessarily respect limits that are applied in
	// our graphql layers. Instead we generate the list from the matches.
	m := c.matchesRepoIDs()
	ids := make([]api.RepoID, 0, len(m))
	for id := range m {
		ids = append(ids, id)
	}
	return c.repositoryResolvers(ctx, ids)
}

func (c *SearchResultsResolver) RepositoriesCount() int32 {
	return int32(len(c.matchesRepoIDs()))
}

func (c *SearchResultsResolver) repositoryResolvers(ctx context.Context, ids []api.RepoID) ([]*RepositoryResolver, error) {
	if len(ids) == 0 {
		return nil, nil
	}

	resolvers := make([]*RepositoryResolver, 0, len(ids))
	err := c.db.Repos().StreamMinimalRepos(ctx, database.ReposListOptions{
		IDs: ids,
	}, func(repo *types.MinimalRepo) {
		resolvers = append(resolvers, NewRepositoryResolver(c.db, repo.ToRepo()))
	})
	if err != nil {
		return nil, err
	}

	sort.Slice(resolvers, func(a, b int) bool {
		return resolvers[a].ID() < resolvers[b].ID()
	})
	return resolvers, nil
}

func (c *SearchResultsResolver) repoIDsByStatus(mask search.RepoStatus) []api.RepoID {
	var ids []api.RepoID
	c.Stats.Status.Filter(mask, func(id api.RepoID) {
		ids = append(ids, id)
	})
	return ids
}

func (c *SearchResultsResolver) Cloning(ctx context.Context) ([]*RepositoryResolver, error) {
	return c.repositoryResolvers(ctx, c.repoIDsByStatus(search.RepoStatusCloning))
}

func (c *SearchResultsResolver) Missing(ctx context.Context) ([]*RepositoryResolver, error) {
	return c.repositoryResolvers(ctx, c.repoIDsByStatus(search.RepoStatusMissing))
}

func (c *SearchResultsResolver) Timedout(ctx context.Context) ([]*RepositoryResolver, error) {
	return c.repositoryResolvers(ctx, c.repoIDsByStatus(search.RepoStatusTimedout))
}

func (c *SearchResultsResolver) IndexUnavailable() bool {
	// This used to return c.Stats.IsIndexUnavailable, but it was never set,
	// so would always return false
	return false
}

// SearchResultsResolver is a resolver for the GraphQL type `SearchResults`
type SearchResultsResolver struct {
	db database.DB
	*SearchResults

	// limit is the maximum number of SearchResults to send back to the user.
	limit int

	// The time it took to compute all results.
	elapsed time.Duration

	// cache for user settings. Ideally this should be set just once in the code path
	// by an upstream resolver
	UserSettings *schema.Settings
}

type SearchResults struct {
	Matches result.Matches
	Stats   streaming.Stats
	Alert   *search.Alert
}

// Results are the results found by the search. It respects the limits set. To
// access all results directly access the SearchResults field.
func (sr *SearchResultsResolver) Results() []SearchResultResolver {
	limited := sr.Matches
	if sr.limit > 0 && sr.limit < len(sr.Matches) {
		limited = sr.Matches[:sr.limit]
	}

	return matchesToResolvers(sr.db, limited)
}

func matchesToResolvers(db database.DB, matches []result.Match) []SearchResultResolver {
	type repoKey struct {
		Name types.MinimalRepo
		Rev  string
	}
	repoResolvers := make(map[repoKey]*RepositoryResolver, 10)
	getRepoResolver := func(repoName types.MinimalRepo, rev string) *RepositoryResolver {
		if existing, ok := repoResolvers[repoKey{repoName, rev}]; ok {
			return existing
		}
		resolver := NewRepositoryResolver(db, repoName.ToRepo())
		resolver.RepoMatch.Rev = rev
		repoResolvers[repoKey{repoName, rev}] = resolver
		return resolver
	}

	resolvers := make([]SearchResultResolver, 0, len(matches))
	for _, match := range matches {
		switch v := match.(type) {
		case *result.FileMatch:
			resolvers = append(resolvers, &FileMatchResolver{
				db:           db,
				FileMatch:    *v,
				RepoResolver: getRepoResolver(v.Repo, ""),
			})
		case *result.RepoMatch:
			resolvers = append(resolvers, getRepoResolver(v.RepoName(), v.Rev))
		case *result.CommitMatch:
			resolvers = append(resolvers, &CommitSearchResultResolver{
				db:          db,
				CommitMatch: *v,
			})
		}
	}
	return resolvers
}

func (sr *SearchResultsResolver) MatchCount() int32 {
	return int32(sr.Matches.ResultCount())
}

// Deprecated. Prefer MatchCount.
func (sr *SearchResultsResolver) ResultCount() int32 { return sr.MatchCount() }

func (sr *SearchResultsResolver) ApproximateResultCount() string {
	count := sr.MatchCount()
	if sr.LimitHit() || sr.Stats.Status.Any(search.RepoStatusCloning|search.RepoStatusTimedout) {
		return fmt.Sprintf("%d+", count)
	}
	return strconv.Itoa(int(count))
}

func (sr *SearchResultsResolver) Alert() *searchAlertResolver {
	return NewSearchAlertResolver(sr.SearchResults.Alert)
}

func (sr *SearchResultsResolver) ElapsedMilliseconds() int32 {
	return int32(sr.elapsed.Milliseconds())
}

func (sr *SearchResultsResolver) DynamicFilters(ctx context.Context) []*searchFilterResolver {
	tr, _ := trace.New(ctx, "DynamicFilters", "", trace.Tag{Key: "resolver", Value: "SearchResultsResolver"})
	defer tr.Finish()

	var filters streaming.SearchFilters
	filters.Update(streaming.SearchEvent{
		Results: sr.Matches,
		Stats:   sr.Stats,
	})

	var resolvers []*searchFilterResolver
	for _, f := range filters.Compute() {
		resolvers = append(resolvers, &searchFilterResolver{filter: *f})
	}
	return resolvers
}

type searchFilterResolver struct {
	filter streaming.Filter
}

func (sf *searchFilterResolver) Value() string {
	return sf.filter.Value
}

func (sf *searchFilterResolver) Label() string {
	return sf.filter.Label
}

func (sf *searchFilterResolver) Count() int32 {
	return int32(sf.filter.Count)
}

func (sf *searchFilterResolver) LimitHit() bool {
	return sf.filter.IsLimitHit
}

func (sf *searchFilterResolver) Kind() string {
	return sf.filter.Kind
}

// blameFileMatch blames the specified file match to produce the time at which
// the first line match inside of it was authored.
func (sr *SearchResultsResolver) blameFileMatch(ctx context.Context, fm *result.FileMatch) (t time.Time, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "blameFileMatch")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	// Blame the first line match.
	if len(fm.LineMatches) == 0 {
		// No line match
		return time.Time{}, nil
	}
	lm := fm.LineMatches[0]
	hunks, err := git.BlameFile(ctx, fm.Repo.Name, fm.Path, &git.BlameOptions{
		NewestCommit: fm.CommitID,
		StartLine:    int(lm.LineNumber),
		EndLine:      int(lm.LineNumber),
	}, authz.DefaultSubRepoPermsChecker)
	if err != nil {
		return time.Time{}, err
	}

	return hunks[0].Author.Date, nil
}

func (sr *SearchResultsResolver) Sparkline(ctx context.Context) (sparkline []int32, err error) {
	var (
		days     = 30                 // number of days the sparkline represents
		maxBlame = 100                // maximum number of file results to blame for date/time information.
		run      = parallel.NewRun(8) // number of concurrent blame ops
	)

	var (
		sparklineMu sync.Mutex
		blameOps    = 0
	)
	sparkline = make([]int32, days)
	addPoint := func(t time.Time) {
		// Check if the author date of the search result is inside of our sparkline
		// timerange.
		now := time.Now()
		if t.Before(now.Add(-time.Duration(len(sparkline)) * 24 * time.Hour)) {
			// Outside the range of the sparkline.
			return
		}
		sparklineMu.Lock()
		defer sparklineMu.Unlock()
		for n := range sparkline {
			d1 := now.Add(-time.Duration(n) * 24 * time.Hour)
			d2 := now.Add(-time.Duration(n-1) * 24 * time.Hour)
			if t.After(d1) && t.Before(d2) {
				sparkline[n]++ // on the nth day
			}
		}
	}

	// Consider all of our search results as a potential data point in our
	// sparkline.
loop:
	for _, r := range sr.Matches {
		r := r // shadow so it doesn't change in the goroutine
		switch m := r.(type) {
		case *result.RepoMatch:
			// We don't care about repo results here.
			continue
		case *result.CommitMatch:
			// Diff searches are cheap, because we implicitly have author date info.
			addPoint(m.Commit.Author.Date)
		case *result.FileMatch:
			// File match searches are more expensive, because we must blame the
			// (first) line in order to know its placement in our sparkline.
			blameOps++
			if blameOps > maxBlame {
				// We have exceeded our budget of blame operations for
				// calculating this sparkline, so don't do any more file match
				// blaming.
				continue loop
			}

			run.Acquire()
			goroutine.Go(func() {
				defer run.Release()

				// Blame the file match in order to retrieve date informatino.
				var err error
				t, err := sr.blameFileMatch(ctx, m)
				if err != nil {
					log15.Warn("failed to blame fileMatch during sparkline generation", "error", err)
					return
				}
				addPoint(t)
			})
		default:
			panic("SearchResults.Sparkline unexpected union type state")
		}
	}
	span := opentracing.SpanFromContext(ctx)
	span.SetTag("blame_ops", blameOps)
	return sparkline, nil
}

var (
	searchResponseCounter = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_graphql_search_response",
		Help: "Number of searches that have ended in the given status (success, error, timeout, partial_timeout).",
	}, []string{"status", "alert_type", "source", "request_name"})

	searchLatencyHistogram = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "src_search_response_latency_seconds",
		Help:    "Search response latencies in seconds that have ended in the given status (success, error, timeout, partial_timeout).",
		Buckets: []float64{0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 20, 30},
	}, []string{"status", "alert_type", "source", "request_name"})
)

// LogSearchLatency records search durations in the event database. This
// function may only be called after a search result is performed, because it
// relies on the invariant that query and pattern error checking has already
// been performed.
func LogSearchLatency(ctx context.Context, db database.DB, wg *sync.WaitGroup, si *run.SearchInputs, durationMs int32) {
	tr, ctx := trace.New(ctx, "LogSearchLatency", "")
	defer tr.Finish()
	var types []string
	resultTypes, _ := si.Query.StringValues(query.FieldType)
	for _, typ := range resultTypes {
		switch typ {
		case "repo", "symbol", "diff", "commit":
			types = append(types, typ)
		case "path":
			// Map type:path to file
			types = append(types, "file")
		case "file":
			switch {
			case si.PatternType == query.SearchTypeStructural:
				types = append(types, "structural")
			case si.PatternType == query.SearchTypeLiteral:
				types = append(types, "literal")
			case si.PatternType == query.SearchTypeRegex:
				types = append(types, "regexp")
			}
		}
	}

	// Don't record composite searches that specify more than one type:
	// because we can't break down the search timings into multiple
	// categories.
	if len(types) > 1 {
		return
	}

	q, err := query.ToBasicQuery(si.Query)
	if err != nil {
		// Can't convert to a basic query, can't guarantee accurate reporting.
		return
	}
	if !query.IsPatternAtom(q) {
		// Not an atomic pattern, can't guarantee accurate reporting.
		return
	}

	// If no type: was explicitly specified, infer the result type.
	if len(types) == 0 {
		// If a pattern was specified, a content search happened.
		if q.IsLiteral() {
			types = append(types, "literal")
		} else if q.IsRegexp() {
			types = append(types, "regexp")
		} else if q.IsStructural() {
			types = append(types, "structural")
		} else if len(si.Query.Fields()["file"]) > 0 {
			// No search pattern specified and file: is specified.
			types = append(types, "file")
		} else {
			// No search pattern or file: is specified, assume repo.
			// This includes accounting for searches of fields that
			// specify repohasfile: and repohascommitafter:.
			types = append(types, "repo")
		}
	}

	// Only log the time if we successfully resolved one search type.
	if len(types) == 1 {
		a := actor.FromContext(ctx)
		if a.IsAuthenticated() && !a.IsMockUser() { // Do not log in tests
			value := fmt.Sprintf(`{"durationMs": %d}`, durationMs)
			eventName := fmt.Sprintf("search.latencies.%s", types[0])
			featureFlags := featureflag.FromContext(ctx)
			wg.Add(1)
			go func() {
				defer wg.Done()
				err := usagestats.LogBackendEvent(db, a.UID, deviceid.FromContext(ctx), eventName, json.RawMessage(value), json.RawMessage(value), featureFlags, nil)
				if err != nil {
					log15.Warn("Could not log search latency", "err", err)
				}
			}()
		}
	}
}

func (r *searchResolver) toRepoOptions(q query.Q) search.RepoOptions {
	repoFilters, minusRepoFilters := q.Repositories()

	var settingForks, settingArchived bool
	if v := r.UserSettings.SearchIncludeForks; v != nil {
		settingForks = *v
	}
	if v := r.UserSettings.SearchIncludeArchived; v != nil {
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
		SearchContextSpec: searchContextSpec,
		UserSettings:      r.UserSettings,
		OnlyForks:         fork == query.Only,
		NoForks:           fork == query.No,
		OnlyArchived:      archived == query.Only,
		NoArchived:        archived == query.No,
		Visibility:        visibility,
		CommitAfter:       commitAfter,
		Query:             q,
	}
}

func withMode(args search.TextParameters, st query.SearchType) search.TextParameters {
	isGlobalSearch := func() bool {
		if st == query.SearchTypeStructural {
			return false
		}

		return query.ForAll(args.Query, func(node query.Node) bool {
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
			case
				query.FieldRepoHasFile:
				return false
			default:
				return true
			}
		})
	}

	hasGlobalSearchResultType := args.ResultTypes.Has(result.TypeFile | result.TypePath | result.TypeSymbol)
	isIndexedSearch := args.PatternInfo.Index != query.No
	isEmpty := args.PatternInfo.Pattern == "" && args.PatternInfo.ExcludePattern == "" && len(args.PatternInfo.IncludePatterns) == 0
	if isGlobalSearch() && isIndexedSearch && hasGlobalSearchResultType && !isEmpty {
		args.Mode = search.ZoektGlobalSearch
	}
	if isEmpty {
		args.Mode = search.SkipUnindexed
	}
	return args
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

// toSearchJob converts a query parse tree to the _internal_ representation
// needed to run a search routine. To understand why this conversion matters, think
// about the fact that the query parse tree doesn't know anything about our
// backends or architecture. It doesn't decide certain defaults, like whether we
// should return multiple result types (pattern matches content, or a file name,
// or a repo name). If we want to optimize a Sourcegraph query parse tree for a
// particular backend (e.g., skip repository resolution and just run a Zoekt
// query on all indexed repositories) then we need to convert our tree to
// Zoekt's internal inputs and representation. These concerns are all handled by
// toSearchJob.
func (r *searchResolver) toSearchJob(q query.Q) (run.Job, error) {
	// MaxResults depends on the query's `count:` parameter, and we should
	// use the passed-in query to do this. However, `r.MaxResults()` uses
	// the query stored on the resolver's SearchInputs rather than the passed-in
	// query. This means for things like `evaluateAnd`, which modify the query
	// count, the query on the resolver does not match the query passed in here,
	// which leads to incorrect counts.
	inputs := *r.SearchInputs // copy search inputs to update q
	inputs.Query = q
	maxResults := inputs.MaxResults()

	b, err := query.ToBasicQuery(q)
	if err != nil {
		return nil, err
	}
	p := search.ToTextPatternInfo(b, r.protocol(), query.Identity)

	forceResultTypes := result.TypeEmpty
	if r.PatternType == query.SearchTypeStructural {
		if p.Pattern == "" {
			// Fallback to literal search for searching repos and files if
			// the structural search pattern is empty.
			r.PatternType = query.SearchTypeLiteral
			p.IsStructuralPat = false
			forceResultTypes = result.Types(0)
		} else {
			forceResultTypes = result.TypeStructural
		}
	}

	args := search.TextParameters{
		PatternInfo: p,
		Query:       q,
		Features:    toFeatures(r.SearchInputs.Features),
		Timeout:     search.TimeoutDuration(b),

		// UseFullDeadline if timeout: set or we are streaming.
		UseFullDeadline: q.Timeout() != nil || q.Count() != nil || r.protocol() == search.Streaming,

		Zoekt:        r.zoekt,
		SearcherURLs: r.searcherURLs,
	}
	args = withResultTypes(args, forceResultTypes)
	args = withMode(args, r.PatternType)
	repoOptions := r.toRepoOptions(args.Query)
	// explicitly populate RepoOptions field in args, because the repo search job
	// still relies on all of args. In time it should depend only on the bits it truly needs.
	args.RepoOptions = repoOptions

	var requiredJobs, optionalJobs []run.Job
	addJob := func(required bool, job run.Job) {
		// Filter out any jobs that aren't commit jobs as they are added
		if inputs.CodeMonitorID != nil {
			if _, ok := job.(*commit.CommitSearch); !ok {
				return
			}
		}

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

		globalSearch := args.Mode == search.ZoektGlobalSearch
		// skipUnindexed is a value that controls whether to run
		// unindexed search in a specific scenario of queries that
		// contain no repo-affecting filters (global mode). When on
		// sourcegraph.com, we resolve only a subset of all indexed
		// repos to search. This control flow implies len(searcherRepos)
		// is always 0, meaning that we should not create jobs to run
		// unindexed searcher.
		skipUnindexed := args.Mode == search.SkipUnindexed || (globalSearch && envvar.SourcegraphDotComMode())
		// searcherOnly is a value that controls whether to run
		// unindexed search in one of two scenarios. The first scenario
		// depends on if index:no is set (value true). The second
		// scenario happens if queries contain no repo-affecting filters
		// (global mode). When NOT on sourcegraph.com the we _may_
		// resolve some subset of nonindexed repos to search, so wemay
		// generate jobs that run searcher, but it is conditional on
		// whether global zoekt search will run (value true).
		searcherOnly := args.Mode == search.SearcherOnly || (globalSearch && !envvar.SourcegraphDotComMode())

		if globalSearch {
			defaultScope, err := zoektutil.DefaultGlobalQueryScope(repoOptions)
			if err != nil {
				return nil, err
			}
			includePrivate := repoOptions.Visibility == query.Private || repoOptions.Visibility == query.Any

			if args.ResultTypes.Has(result.TypeFile | result.TypePath) {
				typ := search.TextRequest
				zoektQuery, err := search.QueryToZoektQuery(args.PatternInfo, &args.Features, typ)
				if err != nil {
					return nil, err
				}

				globalZoektQuery := zoektutil.NewGlobalZoektQuery(zoektQuery, defaultScope, includePrivate)

				zoektArgs := &search.ZoektParameters{
					// TODO(rvantonder): the Query value is set when the global zoekt query is
					// enriched with private repository data in the search job's Run method, and
					// is therefore set to `nil` below.
					// Ideally, The ZoektParameters type should not expose this field for Universe text
					// searches at all, and will be removed once jobs are fully migrated.
					Query:          nil,
					Typ:            typ,
					FileMatchLimit: args.PatternInfo.FileMatchLimit,
					Select:         args.PatternInfo.Select,
					Zoekt:          args.Zoekt,
				}

				addJob(true, &textsearch.RepoUniverseTextSearch{
					GlobalZoektQuery: globalZoektQuery,
					ZoektArgs:        zoektArgs,

					RepoOptions: repoOptions,
				})
			}

			if args.ResultTypes.Has(result.TypeSymbol) {
				typ := search.SymbolRequest
				zoektQuery, err := search.QueryToZoektQuery(args.PatternInfo, &args.Features, typ)
				if err != nil {
					return nil, err
				}
				globalZoektQuery := zoektutil.NewGlobalZoektQuery(zoektQuery, defaultScope, includePrivate)

				zoektArgs := &search.ZoektParameters{
					Query:          nil,
					Typ:            typ,
					FileMatchLimit: args.PatternInfo.FileMatchLimit,
					Select:         args.PatternInfo.Select,
					Zoekt:          args.Zoekt,
				}

				addJob(true, &symbol.RepoUniverseSymbolSearch{
					GlobalZoektQuery: globalZoektQuery,
					ZoektArgs:        zoektArgs,
					PatternInfo:      args.PatternInfo,
					Limit:            maxResults,

					RepoOptions: repoOptions,
				})
			}
		}

		if args.ResultTypes.Has(result.TypeFile | result.TypePath) {
			if !skipUnindexed {
				typ := search.TextRequest
				// TODO(rvantonder): we don't always have to run
				// this converter. It depends on whether we run
				// a zoekt search at all.
				zoektQuery, err := search.QueryToZoektQuery(args.PatternInfo, &args.Features, typ)
				if err != nil {
					return nil, err
				}
				zoektArgs := &search.ZoektParameters{
					Query:          zoektQuery,
					Typ:            typ,
					FileMatchLimit: args.PatternInfo.FileMatchLimit,
					Select:         args.PatternInfo.Select,
					Zoekt:          args.Zoekt,
				}

				searcherArgs := &search.SearcherParameters{
					SearcherURLs:    args.SearcherURLs,
					PatternInfo:     args.PatternInfo,
					UseFullDeadline: args.UseFullDeadline,
				}

				addJob(true, &textsearch.RepoSubsetTextSearch{
					ZoektArgs:        zoektArgs,
					SearcherArgs:     searcherArgs,
					NotSearcherOnly:  !searcherOnly,
					UseIndex:         args.PatternInfo.Index,
					ContainsRefGlobs: query.ContainsRefGlobs(q),
					RepoOpts:         repoOptions,
				})
			}
		}

		if args.ResultTypes.Has(result.TypeSymbol) && args.PatternInfo.Pattern != "" {
			if !skipUnindexed {
				typ := search.SymbolRequest
				zoektQuery, err := search.QueryToZoektQuery(args.PatternInfo, &args.Features, typ)
				if err != nil {
					return nil, err
				}
				zoektArgs := &search.ZoektParameters{
					Query:          zoektQuery,
					Typ:            typ,
					FileMatchLimit: args.PatternInfo.FileMatchLimit,
					Select:         args.PatternInfo.Select,
					Zoekt:          args.Zoekt,
				}

				required := args.UseFullDeadline || args.ResultTypes.Without(result.TypeSymbol) == 0
				addJob(required, &symbol.RepoSubsetSymbolSearch{
					ZoektArgs:        zoektArgs,
					PatternInfo:      args.PatternInfo,
					Limit:            maxResults,
					NotSearcherOnly:  !searcherOnly,
					UseIndex:         args.PatternInfo.Index,
					ContainsRefGlobs: query.ContainsRefGlobs(q),
					RepoOpts:         repoOptions,
				})
			}
		}

		if args.ResultTypes.Has(result.TypeCommit) || args.ResultTypes.Has(result.TypeDiff) {
			diff := args.ResultTypes.Has(result.TypeDiff)
			var required bool
			if args.UseFullDeadline {
				required = true
			} else if diff {
				required = args.ResultTypes.Without(result.TypeDiff) == 0
			} else {
				required = args.ResultTypes.Without(result.TypeCommit) == 0
			}
			addJob(required, &commit.CommitSearch{
				Query:                commit.QueryToGitQuery(args.Query, diff),
				RepoOpts:             repoOptions,
				Diff:                 diff,
				HasTimeFilter:        commit.HasTimeFilter(args.Query),
				Limit:                int(args.PatternInfo.FileMatchLimit),
				CodeMonitorID:        inputs.CodeMonitorID,
				IncludeModifiedFiles: authz.SubRepoEnabled(authz.DefaultSubRepoPermsChecker),
			})
		}

		if r.PatternType == query.SearchTypeStructural && p.Pattern != "" {
			typ := search.TextRequest
			zoektQuery, err := search.QueryToZoektQuery(args.PatternInfo, &args.Features, typ)
			if err != nil {
				return nil, err
			}
			zoektArgs := &search.ZoektParameters{
				Query:          zoektQuery,
				Typ:            typ,
				FileMatchLimit: args.PatternInfo.FileMatchLimit,
				Select:         args.PatternInfo.Select,
				Zoekt:          args.Zoekt,
			}

			searcherArgs := &search.SearcherParameters{
				SearcherURLs:    args.SearcherURLs,
				PatternInfo:     args.PatternInfo,
				UseFullDeadline: args.UseFullDeadline,
			}

			addJob(true, &structural.StructuralSearch{
				ZoektArgs:    zoektArgs,
				SearcherArgs: searcherArgs,

				NotSearcherOnly:  !searcherOnly,
				UseIndex:         args.PatternInfo.Index,
				ContainsRefGlobs: query.ContainsRefGlobs(q),
				RepoOpts:         repoOptions,
			})
		}

		if args.ResultTypes.Has(result.TypeRepo) {
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
				for field := range args.Query.Fields() {
					if _, ok := fieldAllowlist[field]; !ok {
						return false
					}
				}
				return true
			}

			// returns an updated RepoOptions if the pattern part of a query can be used to
			// search repos. A problematic case we check for is when the pattern contains `@`,
			// which may confuse downstream logic to interpret it as part of `repo@rev` syntax.
			addPatternAsRepoFilter := func(pattern string, opts search.RepoOptions) (search.RepoOptions, bool) {
				if pattern == "" {
					return opts, true
				}

				opts.RepoFilters = append(make([]string, 0, len(opts.RepoFilters)), opts.RepoFilters...)
				opts.CaseSensitiveRepoFilters = args.Query.IsCaseSensitive()

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
				if repoOptions, ok := addPatternAsRepoFilter(args.PatternInfo.Pattern, repoOptions); ok {
					args.RepoOptions = repoOptions
					addJob(true, &run.RepoSearch{
						Args: &args,
					})
				}
			}
		}
	}

	addJob(true, &searchrepos.ComputeExcludedRepos{
		Options: repoOptions,
	})

	job := run.NewPriorityJob(
		run.NewParallelJob(requiredJobs...),
		run.NewParallelJob(optionalJobs...),
	)

	checker := authz.DefaultSubRepoPermsChecker
	if authz.SubRepoEnabled(checker) {
		job = run.NewFilterJob(job)
	}

	return job, nil
}

// toAndJob creates a new job from a basic query whose pattern is an And operator at the root.
func (r *searchResolver) toAndJob(q query.Basic) (run.Job, error) {
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

	operands := make([]run.Job, 0, len(queryOperands))
	for _, queryOperand := range queryOperands {
		operand, err := r.toPatternExpressionJob(q.MapPattern(queryOperand))
		if err != nil {
			return nil, err
		}
		operands = append(operands, run.NewLimitJob(maxTryCount, operand))
	}

	return run.NewAndJob(operands...), nil
}

// toOrJob creates a new job from a basic query whose pattern is an Or operator at the top level
func (r *searchResolver) toOrJob(q query.Basic) (run.Job, error) {
	// Invariant: this function is only reachable from callers that
	// guarantee a root node with one or more queryOperands.
	queryOperands := q.Pattern.(query.Operator).Operands

	operands := make([]run.Job, 0, len(queryOperands))
	for _, term := range queryOperands {
		operand, err := r.toPatternExpressionJob(q.MapPattern(term))
		if err != nil {
			return nil, err
		}
		operands = append(operands, operand)
	}
	return run.NewOrJob(operands...), nil
}

func (r *searchResolver) toPatternExpressionJob(q query.Basic) (run.Job, error) {
	switch term := q.Pattern.(type) {
	case query.Operator:
		if len(term.Operands) == 0 {
			return run.NewNoopJob(), nil
		}

		switch term.Kind {
		case query.And:
			return r.toAndJob(q)
		case query.Or:
			return r.toOrJob(q)
		case query.Concat:
			return r.toSearchJob(q.ToParseTree())
		}
	case query.Pattern:
		return r.toSearchJob(q.ToParseTree())
	case query.Parameter:
		// evaluatePatternExpression does not process Parameter nodes.
		return run.NewNoopJob(), nil
	}
	// Unreachable.
	return nil, errors.Errorf("unrecognized type %T in evaluatePatternExpression", q.Pattern)
}

func (r *searchResolver) toEvaluateJob(q query.Basic) (run.Job, error) {
	maxResults := r.MaxResults()
	timeout := search.TimeoutDuration(q)

	if q.Pattern == nil {
		job, err := r.toSearchJob(query.ToNodes(q.Parameters))
		return run.NewTimeoutJob(timeout, run.NewLimitJob(maxResults, job)), err
	}
	job, err := r.toPatternExpressionJob(q)
	return run.NewTimeoutJob(timeout, run.NewLimitJob(maxResults, job)), err
}

// evaluate evaluates all expressions of a search query. The value of stream must be non-nil
func (r *searchResolver) evaluate(ctx context.Context, stream streaming.Sender, q query.Basic) (*search.Alert, error) {
	j, err := r.toEvaluateJob(q)
	if err != nil {
		return nil, err
	}
	return r.evaluateJob(ctx, stream, j)
}

func logPrometheusBatch(status, alertType, requestSource, requestName string, elapsed time.Duration) {
	searchResponseCounter.WithLabelValues(
		status,
		alertType,
		requestSource,
		requestName,
	).Inc()

	searchLatencyHistogram.WithLabelValues(
		status,
		alertType,
		requestSource,
		requestName,
	).Observe(elapsed.Seconds())
}

func (r *searchResolver) logBatch(ctx context.Context, srr *SearchResultsResolver, start time.Time, err error) {
	elapsed := time.Since(start)
	if srr != nil {
		srr.elapsed = elapsed
		var wg sync.WaitGroup
		LogSearchLatency(ctx, r.db, &wg, r.SearchInputs, srr.ElapsedMilliseconds())
		defer wg.Wait()
	}

	var status, alertType string
	status = DetermineStatusForLogs(srr, err)
	if srr != nil && srr.SearchResults.Alert != nil {
		alertType = srr.SearchResults.Alert.PrometheusType
	}
	requestSource := string(trace.RequestSource(ctx))
	requestName := trace.GraphQLRequestName(ctx)
	logPrometheusBatch(status, alertType, requestSource, requestName, elapsed)

	isSlow := time.Since(start) > searchlogs.LogSlowSearchesThreshold()
	if honey.Enabled() || isSlow {
		var n int
		if srr != nil {
			n = len(srr.Matches)
		}
		ev := searchhoney.SearchEvent(ctx, searchhoney.SearchEventArgs{
			OriginalQuery: r.rawQuery(),
			Typ:           requestName,
			Source:        requestSource,
			Status:        status,
			AlertType:     alertType,
			DurationMs:    elapsed.Milliseconds(),
			ResultSize:    n,
			Error:         err,
		})

		_ = ev.Send()

		if isSlow {
			log15.Warn("slow search request", searchlogs.MapToLog15Ctx(ev.Fields())...)
		}
	}
}

func (r *searchResolver) resultsBatch(ctx context.Context) (*SearchResultsResolver, error) {
	start := time.Now()
	sr, err := r.resultsRecursive(ctx, nil, r.Plan)
	srr := r.resultsToResolver(sr)
	r.logBatch(ctx, srr, start, err)
	return srr, err
}

func (r *searchResolver) resultsStreaming(ctx context.Context) (*SearchResultsResolver, error) {
	stream := r.stream
	if !query.IsStreamingCompatible(r.Plan) {
		srr, err := r.resultsBatch(ctx)
		if srr != nil {
			stream.Send(streaming.SearchEvent{
				Results: srr.Matches,
				Stats:   srr.Stats,
			})
		}
		return srr, err
	}
	if sp, _ := r.Plan.ToParseTree().StringValue(query.FieldSelect); sp != "" {
		// Ensure downstream events sent on the stream are processed by `select:`.
		selectPath, _ := filter.SelectPathFromString(sp) // Invariant: error already checked
		stream = streaming.WithSelect(stream, selectPath)
	}
	sr, err := r.resultsRecursive(ctx, stream, r.Plan)
	srr := r.resultsToResolver(sr)
	return srr, err
}

func (r *searchResolver) resultsToResolver(results *SearchResults) *SearchResultsResolver {
	if results == nil {
		results = &SearchResults{}
	}
	return &SearchResultsResolver{
		SearchResults: results,
		limit:         r.MaxResults(),
		db:            r.db,
		UserSettings:  r.UserSettings,
	}
}

func (r *searchResolver) Results(ctx context.Context) (*SearchResultsResolver, error) {
	if r.stream == nil {
		return r.resultsBatch(ctx)
	}
	return r.resultsStreaming(ctx)
}

// DetermineStatusForLogs determines the final status of a search for logging
// purposes.
func DetermineStatusForLogs(srr *SearchResultsResolver, err error) string {
	switch {
	case err == context.DeadlineExceeded:
		return "timeout"
	case err != nil:
		return "error"
	case srr.Stats.Status.All(search.RepoStatusTimedout) && srr.Stats.Status.Len() == len(srr.Stats.Repos):
		return "timeout"
	case srr.Stats.Status.Any(search.RepoStatusTimedout):
		return "partial_timeout"
	case srr.SearchResults.Alert != nil:
		return "alert"
	default:
		return "success"
	}
}

func (r *searchResolver) resultsRecursive(ctx context.Context, stream streaming.Sender, plan query.Plan) (_ *SearchResults, err error) {
	tr, ctx := trace.New(ctx, "Results", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	wantCount := defaultMaxSearchResults
	if count := r.Query.Count(); count != nil {
		wantCount = *count
	}

	var (
		mu     sync.Mutex
		stats  streaming.Stats
		alerts []*search.Alert
		dedup  = result.NewDeduper()
		// NOTE(tsenart): In the future, when we have the need for more intelligent rate limiting,
		// this concurrency limit should probably be informed by a user's rate limit quota
		// at any given time.
		sem = semaphore.NewWeighted(16)
	)

	g, ctx := errgroup.WithContext(ctx)
	for _, q := range plan {
		q := q
		g.Go(func() error {
			if err := sem.Acquire(ctx, 1); err != nil {
				return err
			}

			defer sem.Release(1)

			predicatePlan, err := substitutePredicates(q, func(pred query.Predicate) (*SearchResults, error) {
				plan, err := pred.Plan(q)
				if err != nil {
					return nil, err
				}
				// Pass a nil stream for subqueries so we can use
				// the results rather than sending them back to the caller
				return r.resultsRecursive(ctx, nil, plan)
			})
			if errors.Is(err, ErrPredicateNoResults) {
				return nil
			}
			if err != nil {
				// Fail if predicate processing fails.
				return err
			}

			var newResult *SearchResults
			if predicatePlan != nil {
				// If a predicate filter generated a new plan, evaluate that plan.
				newResult, err = r.resultsRecursive(ctx, stream, predicatePlan)
			} else if stream != nil {
				var alert *search.Alert
				alert, err = r.evaluate(ctx, stream, q)
				newResult = &SearchResults{Alert: alert}
			} else {
				// Always pass a non-nil stream to evaluate
				agg := streaming.NewAggregatingStream()
				var alert *search.Alert
				alert, err = r.evaluate(ctx, agg, q)
				newResult = &SearchResults{
					Matches: agg.Results,
					Stats:   agg.Stats,
					Alert:   alert,
				}
			}

			if err != nil || newResult == nil {
				// Fail if any subexpression fails.
				return err
			}

			var selectMatch func(result.Match) result.Match
			if v, _ := q.ToParseTree().StringValue(query.FieldSelect); v != "" {
				sp, _ := filter.SelectPathFromString(v) // Invariant: select already validated
				selectMatch = func(m result.Match) result.Match {
					return m.Select(sp)
				}
			} else {
				selectMatch = func(m result.Match) result.Match {
					return m
				}
			}

			mu.Lock()
			defer mu.Unlock()

			if newResult.Alert != nil {
				alerts = append(alerts, newResult.Alert)
			}

			// Check if another go-routine has already produced enough results.
			if wantCount <= 0 {
				return context.Canceled
			}

			// BUG: When we find enough results we stop adding them to dedupper,
			// but don't adjust the stats accordingly. This bug was here
			// before, and remains after making query evaluation concurrent.
			stats.Update(&newResult.Stats)

			for _, m := range newResult.Matches {
				match := selectMatch(m)

				if match == nil {
					continue
				}

				wantCount = match.Limit(wantCount)

				if dedup.Add(match); wantCount <= 0 {
					return context.Canceled
				}
			}

			return nil
		})
	}

	if err := g.Wait(); err != nil && err != context.Canceled {
		return nil, err
	}

	matches := dedup.Results()
	if len(matches) > 0 {
		sort.Sort(matches)
	}

	var alert *search.Alert
	if len(alerts) > 0 {
		sort.Slice(alerts, func(i, j int) bool {
			return alerts[i].Priority > alerts[j].Priority
		})
		alert = alerts[0]
	}

	return &SearchResults{
		Matches: matches,
		Stats:   stats,
		Alert:   alert,
	}, err
}

// searchResultsToRepoNodes converts a set of search results into repository nodes
// such that they can be used to replace a repository predicate
func searchResultsToRepoNodes(matches []result.Match) ([]query.Node, error) {
	nodes := make([]query.Node, 0, len(matches))
	for _, match := range matches {
		repoMatch, ok := match.(*result.RepoMatch)
		if !ok {
			return nil, errors.Errorf("expected type %T, but got %T", &result.RepoMatch{}, match)
		}

		repoFieldValue := "^" + regexp.QuoteMeta(string(repoMatch.Name)) + "$"
		if repoMatch.Rev != "" {
			repoFieldValue += "@" + repoMatch.Rev
		}

		nodes = append(nodes, query.Parameter{
			Field: query.FieldRepo,
			Value: repoFieldValue,
		})
	}

	return nodes, nil
}

// searchResultsToFileNodes converts a set of search results into repo/file nodes so that they
// can replace a file predicate
func searchResultsToFileNodes(matches []result.Match) ([]query.Node, error) {
	nodes := make([]query.Node, 0, len(matches))
	for _, match := range matches {
		fileMatch, ok := match.(*result.FileMatch)
		if !ok {
			return nil, errors.Errorf("expected type %T, but got %T", &result.FileMatch{}, match)
		}

		repoFieldValue := "^" + regexp.QuoteMeta(string(fileMatch.Repo.Name)) + "$"
		if fileMatch.InputRev != nil {
			repoFieldValue += "@" + *fileMatch.InputRev
		}

		// We create AND nodes to match both the repo and the file at the same time so
		// we don't get files of the same name from different repositories.
		nodes = append(nodes, query.Operator{
			Kind: query.And,
			Operands: []query.Node{
				query.Parameter{
					Field: query.FieldRepo,
					Value: repoFieldValue,
				},
				query.Parameter{
					Field: query.FieldFile,
					Value: "^" + regexp.QuoteMeta(fileMatch.Path) + "$",
				},
			},
		})
	}

	return nodes, nil
}

// evaluateJob is a toplevel function that runs a search job to yield results.
// A search job represents a tree of evaluation steps. If the deadline
// is exceeded, returns a search alert with a did-you-mean link for the same
// query with a longer timeout.
func (r *searchResolver) evaluateJob(ctx context.Context, stream streaming.Sender, job run.Job) (_ *search.Alert, err error) {
	tr, ctx := trace.New(ctx, "evaluateJob", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()
	tr.LazyPrintf("job name: %s", job.Name())

	start := time.Now()
	countingStream := streaming.NewResultCountingStream(stream)
	statsObserver := streaming.NewStatsObservingStream(countingStream)
	jobAlert, err := job.Run(ctx, r.db, statsObserver)

	ao := alert.Observer{
		Db:           r.db,
		SearchInputs: r.SearchInputs,
		HasResults:   countingStream.Count() > 0,
	}
	if err != nil {
		ao.Error(ctx, err)
	}
	observerAlert, err := ao.Done()

	// We have an alert for context timeouts and we have a progress
	// notification for timeouts. We don't want to show both, so we only show
	// it if no repos are marked as timedout. This somewhat couples us to how
	// progress notifications work, but this is the third attempt at trying to
	// fix this behaviour so we are accepting that.
	if errors.Is(err, context.DeadlineExceeded) {
		if !statsObserver.Status.Any(search.RepoStatusTimedout) {
			usedTime := time.Since(start)
			suggestTime := longer(2, usedTime)
			return search.AlertForTimeout(usedTime, suggestTime, r.rawQuery(), r.PatternType), nil
		} else {
			err = nil
		}
	}

	return search.MaxPriorityAlert(jobAlert, observerAlert), err
}

// substitutePredicates replaces all the predicates in a query with their expanded form. The predicates
// are expanded using the doExpand function.
func substitutePredicates(q query.Basic, evaluate func(query.Predicate) (*SearchResults, error)) (query.Plan, error) {
	var topErr error
	success := false
	newQ := query.MapParameter(q.ToParseTree(), func(field, value string, neg bool, ann query.Annotation) query.Node {
		orig := query.Parameter{
			Field:      field,
			Value:      value,
			Negated:    neg,
			Annotation: ann,
		}

		if !ann.Labels.IsSet(query.IsPredicate) {
			return orig
		}

		if topErr != nil {
			return orig
		}

		name, params := query.ParseAsPredicate(value)
		predicate := query.DefaultPredicateRegistry.Get(field, name)
		predicate.ParseParams(params)
		srr, err := evaluate(predicate)
		if err != nil {
			topErr = err
			return nil
		}

		var nodes []query.Node
		switch predicate.Field() {
		case query.FieldRepo:
			nodes, err = searchResultsToRepoNodes(srr.Matches)
			if err != nil {
				topErr = err
				return nil
			}
		case query.FieldFile:
			nodes, err = searchResultsToFileNodes(srr.Matches)
			if err != nil {
				topErr = err
				return nil
			}
		default:
			topErr = errors.Errorf("unsupported predicate result type %q", predicate.Field())
			return nil
		}

		// If no results are returned, we need to return a sentinel error rather
		// than an empty expansion because an empty expansion means "everything"
		// rather than "nothing".
		if len(nodes) == 0 {
			topErr = ErrPredicateNoResults
			return nil
		}

		// A predicate was successfully evaluated and has results.
		success = true

		// No need to return an operator for only one result
		if len(nodes) == 1 {
			return nodes[0]
		}

		return query.Operator{
			Kind:     query.Or,
			Operands: nodes,
		}
	})

	if topErr != nil || !success {
		return nil, topErr
	}
	plan, err := query.ToPlan(query.Dnf(newQ))
	if err != nil {
		return nil, err
	}
	return plan, nil
}

var ErrPredicateNoResults = errors.New("no results returned for predicate")

// longer returns a suggested longer time to wait if the given duration wasn't long enough.
func longer(n int, dt time.Duration) time.Duration {
	dt2 := func() time.Duration {
		Ndt := time.Duration(n) * dt
		dceil := func(x float64) time.Duration {
			return time.Duration(math.Ceil(x))
		}
		switch {
		case math.Floor(Ndt.Hours()) > 0:
			return dceil(Ndt.Hours()) * time.Hour
		case math.Floor(Ndt.Minutes()) > 0:
			return dceil(Ndt.Minutes()) * time.Minute
		case math.Floor(Ndt.Seconds()) > 0:
			return dceil(Ndt.Seconds()) * time.Second
		default:
			return 0
		}
	}()
	lowest := 2 * time.Second
	if dt2 < lowest {
		return lowest
	}
	return dt2
}

type searchResultsStats struct {
	JApproximateResultCount string
	JSparkline              []int32

	sr *searchResolver

	// These items are lazily populated by getResults
	once    sync.Once
	results result.Matches
	err     error
}

func (srs *searchResultsStats) ApproximateResultCount() string { return srs.JApproximateResultCount }
func (srs *searchResultsStats) Sparkline() []int32             { return srs.JSparkline }

var (
	searchResultsStatsCache   = rcache.NewWithTTL("search_results_stats", 3600) // 1h
	searchResultsStatsCounter = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_graphql_search_results_stats_cache_hit",
		Help: "Counts cache hits and misses for search results stats (e.g. sparklines).",
	}, []string{"type"})
)

func (r *searchResolver) Stats(ctx context.Context) (stats *searchResultsStats, err error) {
	// Override user context to ensure that stats for this query are cached
	// regardless of the user context's cancellation. For example, if
	// stats/sparklines are slow to load on the homepage and all users navigate
	// away from that page before they load, no user would ever see them and we
	// would never cache them. This fixes that by ensuring the first request
	// 'kicks off loading' and places the result into cache regardless of
	// whether or not the original querier of this information still wants it.
	originalCtx := ctx
	ctx = context.Background()
	ctx = opentracing.ContextWithSpan(ctx, opentracing.SpanFromContext(originalCtx))

	cacheKey := r.rawQuery()
	// Check if value is in the cache.
	jsonRes, ok := searchResultsStatsCache.Get(cacheKey)
	if ok {
		searchResultsStatsCounter.WithLabelValues("hit").Inc()
		if err := json.Unmarshal(jsonRes, &stats); err != nil {
			return nil, err
		}
		stats.sr = r
		return stats, nil
	}

	// Calculate value from scratch.
	searchResultsStatsCounter.WithLabelValues("miss").Inc()
	attempts := 0
	var v *SearchResultsResolver
	for {
		// Query search results.
		var err error
		job, err := r.toSearchJob(r.Query)
		if err != nil {
			return nil, err
		}
		agg := streaming.NewAggregatingStream()
		_, err = job.Run(ctx, r.db, agg)
		if err != nil {
			return nil, err // do not cache errors.
		}
		v = r.resultsToResolver(&SearchResults{
			Matches: agg.Results,
			Stats:   agg.Stats,
		})
		if v.MatchCount() > 0 {
			break
		}

		status := v.Stats.Status
		if !status.Any(search.RepoStatusCloning) && !status.Any(search.RepoStatusTimedout) {
			break // zero results, but no cloning or timed out repos. No point in retrying.
		}

		var cloning, timedout int
		status.Filter(search.RepoStatusCloning, func(api.RepoID) {
			cloning++
		})
		status.Filter(search.RepoStatusTimedout, func(api.RepoID) {
			timedout++
		})

		if attempts > 5 {
			log15.Error("failed to generate sparkline due to cloning or timed out repos", "cloning", cloning, "timedout", timedout)
			return nil, errors.Errorf("failed to generate sparkline due to %d cloning %d timedout repos", cloning, timedout)
		}

		// We didn't find any search results. Some repos are cloning or timed
		// out, so try again in a few seconds.
		attempts++
		log15.Warn("sparkline generation found 0 search results due to cloning or timed out repos (retrying in 5s)", "cloning", cloning, "timedout", timedout)
		time.Sleep(5 * time.Second)
	}

	sparkline, err := v.Sparkline(ctx)
	if err != nil {
		return nil, err // sparkline generation failed, so don't cache.
	}
	stats = &searchResultsStats{
		JApproximateResultCount: v.ApproximateResultCount(),
		JSparkline:              sparkline,
		sr:                      r,
	}

	// Store in the cache if we got non-zero results. If we got zero results,
	// it should be quick and caching is not desired because e.g. it could be
	// a query for a repo that has not been added by the user yet.
	if v.ResultCount() > 0 {
		jsonRes, err = json.Marshal(stats)
		if err != nil {
			return nil, err
		}
		searchResultsStatsCache.Set(cacheKey, jsonRes)
	}
	return stats, nil
}

// withResultTypes populates the ResultTypes field of args, which drives the kind
// of search to run (e.g., text search, symbol search).
func withResultTypes(args search.TextParameters, forceTypes result.Types) search.TextParameters {
	var rts result.Types
	if forceTypes != 0 {
		rts = forceTypes
	} else {
		stringTypes, _ := args.Query.StringValues(query.FieldType)
		if len(stringTypes) == 0 {
			rts = result.TypeFile | result.TypePath | result.TypeRepo
		} else {
			for _, stringType := range stringTypes {
				rts = rts.With(result.TypeFromString[stringType])
			}
		}
	}

	if rts.Has(result.TypeFile) {
		args.PatternInfo.PatternMatchesContent = true
	}

	if rts.Has(result.TypePath) {
		args.PatternInfo.PatternMatchesPath = true
	}
	args.ResultTypes = rts
	return args
}

// isContextError returns true if ctx.Err() is not nil or if err
// is an error caused by context cancelation or timeout.
func isContextError(ctx context.Context, err error) bool {
	return ctx.Err() != nil || errors.IsAny(err, context.Canceled, context.DeadlineExceeded)
}

// SearchResultResolver is a resolver for the GraphQL union type `SearchResult`.
//
// Supported types:
//
//   - *RepositoryResolver         // repo name match
//   - *fileMatchResolver          // text match
//   - *commitSearchResultResolver // diff or commit match
//
// Note: Any new result types added here also need to be handled properly in search_results.go:301 (sparklines)
type SearchResultResolver interface {
	ToRepository() (*RepositoryResolver, bool)
	ToFileMatch() (*FileMatchResolver, bool)
	ToCommitSearchResult() (*CommitSearchResultResolver, bool)

	ResultCount() int32
}

var metricFeatureFlagUnavailable = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_search_featureflag_unavailable",
	Help: "temporary counter to check if we have feature flag available in practice.",
})
