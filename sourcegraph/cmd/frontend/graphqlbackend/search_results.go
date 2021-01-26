package graphqlbackend

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"path"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/go-enry/go-enry/v2"
	"github.com/hashicorp/go-multierror"
	"github.com/inconshreveable/log15"
	"github.com/neelance/parallel"
	"github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/inventory"
	searchrepos "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/comby"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/honey"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/usagestats"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
)

func (c *SearchResultsResolver) LimitHit() bool {
	return c.IsLimitHit || (c.limit > 0 && len(c.SearchResults) > c.limit)
}

func (c *SearchResultsResolver) Repositories() []*RepositoryResolver {
	repos := c.Repos
	resolvers := make([]*RepositoryResolver, 0, len(repos))
	for _, r := range repos {
		resolvers = append(resolvers, &RepositoryResolver{innerRepo: r.ToRepo()})
	}
	sort.Slice(resolvers, func(a, b int) bool {
		return resolvers[a].innerRepo.ID < resolvers[b].innerRepo.ID
	})
	return resolvers
}

func (c *SearchResultsResolver) RepositoriesCount() int32 {
	return int32(len(c.Repos))
}

func (c *SearchResultsResolver) repositoryResolvers(mask search.RepoStatus) []*RepositoryResolver {
	var resolvers []*RepositoryResolver
	c.Status.Filter(mask, func(id api.RepoID) {
		r := c.Repos[id]
		if r != nil {
			resolvers = append(resolvers, &RepositoryResolver{innerRepo: c.Repos[id].ToRepo()})
		}
	})
	sort.Slice(resolvers, func(a, b int) bool {
		return resolvers[a].innerRepo.ID < resolvers[b].innerRepo.ID
	})
	return resolvers
}

func (c *SearchResultsResolver) RepositoriesSearched() []*RepositoryResolver {
	return c.repositoryResolvers(search.RepoStatusSearched)
}

func (c *SearchResultsResolver) IndexedRepositoriesSearched() []*RepositoryResolver {
	return c.repositoryResolvers(search.RepoStatusIndexed)
}

func (c *SearchResultsResolver) Cloning() []*RepositoryResolver {
	return c.repositoryResolvers(search.RepoStatusCloning)
}

func (c *SearchResultsResolver) Missing() []*RepositoryResolver {
	return c.repositoryResolvers(search.RepoStatusMissing)
}

func (c *SearchResultsResolver) Timedout() []*RepositoryResolver {
	return c.repositoryResolvers(search.RepoStatusTimedout)
}

func (c *SearchResultsResolver) IndexUnavailable() bool {
	return c.IsIndexUnavailable
}

func (c *SearchResultsResolver) allReposTimedout() bool {
	return c.Status.All(search.RepoStatusTimedout) && c.Status.Len() == len(c.Repos)
}

// SearchResultsResolver is a resolver for the GraphQL type `SearchResults`
type SearchResultsResolver struct {
	// SearchResults is the full list of results found. The method Results()
	// will return the list respecting limits.
	SearchResults []SearchResultResolver
	streaming.Stats

	// limit is the maximum number of SearchResults to send back to the user.
	limit int

	alert *searchAlert
	start time.Time // when the results started being computed

	// cursor to return for paginated search requests, or nil if the request
	// wasn't paginated.
	cursor *searchCursor

	// cache for user settings. Ideally this should be set just once in the code path
	// by an upstream resolver
	UserSettings *schema.Settings
}

// Results are the results found by the search. It respects the limits set. To
// access all results directly access the SearchResults field.
func (sr *SearchResultsResolver) Results() []SearchResultResolver {
	if sr.limit > 0 && sr.limit < len(sr.SearchResults) {
		return sr.SearchResults[:sr.limit]
	}

	return sr.SearchResults
}

func (sr *SearchResultsResolver) MatchCount() int32 {
	var totalResults int32
	for _, result := range sr.SearchResults {
		totalResults += result.ResultCount()
	}
	return totalResults
}

// Deprecated. Prefer MatchCount.
func (sr *SearchResultsResolver) ResultCount() int32 { return sr.MatchCount() }

func (sr *SearchResultsResolver) ApproximateResultCount() string {
	count := sr.MatchCount()
	if sr.LimitHit() || sr.Status.Any(search.RepoStatusCloning|search.RepoStatusTimedout) {
		return fmt.Sprintf("%d+", count)
	}
	return strconv.Itoa(int(count))
}

func (sr *SearchResultsResolver) Alert() *searchAlert { return sr.alert }

func (sr *SearchResultsResolver) ElapsedMilliseconds() int32 {
	return int32(time.Since(sr.start).Milliseconds())
}

// commonFileFilters are common filters used. It is used by DynamicFilters to
// propose them if they match shown results.
var commonFileFilters = []struct {
	regexp      *lazyregexp.Regexp
	regexFilter string
	globFilter  string
}{
	// Exclude go tests
	{
		regexp:      lazyregexp.New(`_test\.go$`),
		regexFilter: `-file:_test\.go$`,
		globFilter:  `-file:**_test.go`,
	},
	// Exclude go vendor
	{
		regexp:      lazyregexp.New(`(^|/)vendor/`),
		regexFilter: `-file:(^|/)vendor/`,
		globFilter:  `-file:vendor/** -file:**/vendor/**`,
	},
	// Exclude node_modules
	{
		regexp:      lazyregexp.New(`(^|/)node_modules/`),
		regexFilter: `-file:(^|/)node_modules/`,
		globFilter:  `-file:node_modules/** -file:**/node_modules/**`,
	},
}

func (sr *SearchResultsResolver) DynamicFilters(ctx context.Context) []*searchFilterResolver {
	tr, ctx := trace.New(ctx, "DynamicFilters", "", trace.Tag{Key: "resolver", Value: "SearchResultsResolver"})
	defer func() {
		tr.Finish()
	}()

	globbing := false
	// For search, sr.userSettings is set in (r *searchResolver) Results(ctx
	// context.Context). However we might regress on that or call DynamicFilters from
	// other code paths. Hence we fallback to accessing the user settings directly.
	if sr.UserSettings != nil {
		globbing = getBoolPtr(sr.UserSettings.SearchGlobbing, false)
	} else {
		settings, err := decodedViewerFinalSettings(ctx)
		if err != nil {
			log15.Warn("DynamicFilters: could not get user settings from database")
		} else {
			globbing = getBoolPtr(settings.SearchGlobbing, false)
		}
	}
	tr.LogFields(otlog.Bool("globbing", globbing))

	filters := map[string]*streaming.Filter{}
	repoToMatchCount := make(map[string]int32)
	add := func(value string, label string, count int32, limitHit bool, kind string) {
		sf, ok := filters[value]
		if !ok {
			sf = &streaming.Filter{
				Value:      value,
				Label:      label,
				Count:      int(count),
				IsLimitHit: limitHit,
				Kind:       kind,
			}
			filters[value] = sf
		} else {
			sf.Count = int(count)
		}
	}
	important := func(value string) {
		filters[value].Important = true
	}

	addRepoFilter := func(repo *RepositoryResolver, rev string, lineMatchCount int32) {
		uri := repo.Name()
		var filter string
		if globbing {
			filter = fmt.Sprintf(`repo:%s`, uri)
		} else {
			filter = fmt.Sprintf(`repo:^%s$`, regexp.QuoteMeta(uri))
		}

		if rev != "" {
			// We don't need to quote rev. The only special characters we interpret
			// are @ and :, both of which are disallowed in git refs
			filter = filter + fmt.Sprintf(`@%s`, rev)
		}
		limitHit := sr.Stats.Status.Get(repo.IDInt32())&search.RepoStatusLimitHit != 0
		// Increment number of matches per repo. Add will override previous entry for uri
		repoToMatchCount[uri] += lineMatchCount
		add(filter, uri, repoToMatchCount[uri], limitHit, "repo")
	}

	addFileFilter := func(fileMatchPath string, lineMatchCount int32, limitHit bool) {
		for _, ff := range commonFileFilters {
			// use regexp to match file paths unconditionally, whether globbing is enabled or not,
			// since we have no native library call to match `**` for globs.
			if ff.regexp.MatchString(fileMatchPath) {
				if globbing {
					add(ff.globFilter, ff.globFilter, lineMatchCount, limitHit, "file")
				} else {
					add(ff.regexFilter, ff.regexFilter, lineMatchCount, limitHit, "file")
				}
			}
		}
	}

	addLangFilter := func(fileMatchPath string, lineMatchCount int32, limitHit bool) {
		extensionToLanguageLookup := func(path string) string {
			language, _ := inventory.GetLanguageByFilename(path)
			return strings.ToLower(language)
		}
		if ext := path.Ext(fileMatchPath); ext != "" {
			language := extensionToLanguageLookup(fileMatchPath)
			if language != "" {
				if strings.Contains(language, " ") {
					language = strconv.Quote(language)
				}
				value := fmt.Sprintf(`lang:%s`, language)
				add(value, value, lineMatchCount, limitHit, "lang")
			}
		}
	}

	if sr.Stats.ExcludedForks > 0 {
		add("fork:yes", "fork:yes", int32(sr.Stats.ExcludedForks), sr.IsLimitHit, "repo")
		important("fork:yes")
	}
	if sr.Stats.ExcludedArchived > 0 {
		add("archived:yes", "archived:yes", int32(sr.Stats.ExcludedArchived), sr.IsLimitHit, "repo")
		important("archived:yes")
	}
	for _, result := range sr.SearchResults {
		if fm, ok := result.ToFileMatch(); ok {
			rev := ""
			if fm.InputRev != nil {
				rev = *fm.InputRev
			}
			lines := fm.ResultCount()
			addRepoFilter(fm.Repo, rev, lines)
			addLangFilter(fm.path(), lines, fm.LimitHit())
			addFileFilter(fm.path(), lines, fm.LimitHit())

			if len(fm.symbols) > 0 {
				add("type:symbol", "type:symbol", 1, fm.LimitHit(), "symbol")
			}
		} else if r, ok := result.ToRepository(); ok {
			// It should be fine to leave this blank since revision specifiers
			// can only be used with the 'repo:' scope. In that case,
			// we shouldn't be getting any repositoy name matches back.
			addRepoFilter(r, "", 1)
		}
	}

	filterSlice := make([]*streaming.Filter, 0, len(filters))
	repoFilterSlice := make([]*streaming.Filter, 0, len(filters)/2) // heuristic - half of all filters are repo filters.
	for _, f := range filters {
		if f.Kind == "repo" {
			repoFilterSlice = append(repoFilterSlice, f)
		} else {
			filterSlice = append(filterSlice, f)
		}
	}
	sort.Slice(filterSlice, func(i, j int) bool {
		if filterSlice[i].Important == filterSlice[j].Important {
			return filterSlice[i].Count > filterSlice[j].Count
		}
		return filterSlice[i].Important
	})
	// limit amount of non-repo filters to be rendered arbitrarily to 12
	if len(filterSlice) > 12 {
		filterSlice = filterSlice[:12]
	}

	allFilters := append(filterSlice, repoFilterSlice...)
	sort.Slice(allFilters, func(i, j int) bool {
		left := allFilters[i]
		right := allFilters[j]
		if left.Important == right.Important {
			// Order alphabetically for equal scores.
			return strings.Compare(left.Value, right.Value) < 0
		}
		return left.Important
	})

	var resolvers []*searchFilterResolver
	for _, f := range allFilters {
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
func (sr *SearchResultsResolver) blameFileMatch(ctx context.Context, fm *FileMatchResolver) (t time.Time, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "blameFileMatch")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	// Blame the first line match.
	lineMatches := fm.LineMatches()
	if len(lineMatches) == 0 {
		// No line match
		return time.Time{}, nil
	}
	lm := fm.LineMatches()[0]
	hunks, err := git.BlameFile(ctx, fm.Repo.innerRepo.Name, fm.path(), &git.BlameOptions{
		NewestCommit: fm.CommitID,
		StartLine:    int(lm.LineNumber()),
		EndLine:      int(lm.LineNumber()),
	})
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
	for _, r := range sr.SearchResults {
		r := r // shadow so it doesn't change in the goroutine
		switch m := r.(type) {
		case *RepositoryResolver:
			// We don't care about repo results here.
			continue
		case *CommitSearchResultResolver:
			// Diff searches are cheap, because we implicitly have author date info.
			addPoint(m.commit.commit.Author.Date)
		case *FileMatchResolver:
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

var searchResponseCounter = promauto.NewCounterVec(prometheus.CounterOpts{
	Name: "src_graphql_search_response",
	Help: "Number of searches that have ended in the given status (success, error, timeout, partial_timeout).",
}, []string{"status", "alert_type", "source", "request_name"})

// logSearchLatency records search durations in the event database. This
// function may only be called after a search result is performed, because it
// relies on the invariant that query and pattern error checking has already
// been performed.
func (r *searchResolver) logSearchLatency(ctx context.Context, durationMs int32) {
	tr, ctx := trace.New(ctx, "logSearchLatency", "")
	defer func() {
		tr.Finish()
	}()
	var types []string
	resultTypes, _ := r.query.StringValues(query.FieldType)
	for _, typ := range resultTypes {
		switch typ {
		case "repo", "symbol", "diff", "commit":
			types = append(types, typ)
		case "path":
			// Map type:path to file
			types = append(types, "file")
		case "file":
			switch {
			case r.patternType == query.SearchTypeStructural:
				types = append(types, "structural")
			case r.patternType == query.SearchTypeLiteral:
				types = append(types, "literal")
			case r.patternType == query.SearchTypeRegex:
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

	options := &getPatternInfoOptions{}
	if r.patternType == query.SearchTypeStructural {
		options = &getPatternInfoOptions{performStructuralSearch: true}
	}
	if r.patternType == query.SearchTypeLiteral {
		options = &getPatternInfoOptions{performLiteralSearch: true}
	}
	p, _ := r.getPatternInfo(options)

	// If no type: was explicitly specified, infer the result type.
	if len(types) == 0 {
		// If a pattern was specified, a content search happened.
		if p.Pattern != "" {
			switch {
			case r.patternType == query.SearchTypeStructural:
				types = append(types, "structural")
			case r.patternType == query.SearchTypeLiteral:
				types = append(types, "literal")
			case r.patternType == query.SearchTypeRegex:
				types = append(types, "regexp")
			}
		} else if len(r.query.Fields()["file"]) > 0 {
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
		actor := actor.FromContext(ctx)
		if actor.IsAuthenticated() {
			value := fmt.Sprintf(`{"durationMs": %d}`, durationMs)
			eventName := fmt.Sprintf("search.latencies.%s", types[0])
			go func() {
				err := usagestats.LogBackendEvent(actor.UID, eventName, json.RawMessage(value))
				if err != nil {
					log15.Warn("Could not log search latency", "err", err)
				}
			}()
		}
	}
}

// evaluateLeaf performs a single search operation and corresponds to the
// evaluation of leaf expression in a query.
func (r *searchResolver) evaluateLeaf(ctx context.Context) (_ *SearchResultsResolver, err error) {
	tr, ctx := trace.New(ctx, "evaluateLeaf", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()
	start := time.Now()
	// If the request specifies stable:truthy, use pagination to return a stable ordering.
	if r.query.BoolValue("stable") {
		result, err := r.paginatedResults(ctx)
		if err != nil {
			return nil, err
		}
		if result == nil {
			// Panic if paginatedResults does not ensure a non-nil search result.
			panic("stable search: paginated search returned nil results")
		}
		if result.cursor == nil {
			// Perhaps an alert was raised.
			return result, err
		}
		if !result.cursor.Finished {
			// For stable result queries limitHit = true implies
			// there is a next cursor, and more results may exist.
			result.Stats.IsLimitHit = true
		}
		return result, err
	}

	// If the request is a paginated one, we handle it separately. See
	// paginatedResults for more details.
	if r.pagination != nil {
		return r.paginatedResults(ctx)
	}

	rr, err := r.resultsWithTimeoutSuggestion(ctx)
	if rr != nil {
		r.logSearchLatency(ctx, rr.ElapsedMilliseconds())
	}

	// Record what type of response we sent back via Prometheus.
	var status, alertType string
	switch {
	case err == context.DeadlineExceeded || (err == nil && rr.allReposTimedout()):
		status = "timeout"
	case err == nil && rr.Stats.Status.Any(search.RepoStatusTimedout):
		status = "partial_timeout"
	case err == nil && rr.alert != nil:
		status = "alert"
		alertType = rr.alert.prometheusType
	case err != nil:
		status = "error"
	case err == nil:
		status = "success"
	default:
		status = "unknown"
	}
	searchResponseCounter.WithLabelValues(
		status,
		alertType,
		string(trace.RequestSource(ctx)),
		trace.GraphQLRequestName(ctx),
	).Inc()

	isSlow := time.Since(start) > logSlowSearchesThreshold()
	if honey.Enabled() || isSlow {
		var act actor.Actor
		if a := actor.FromContext(ctx); a != nil {
			act = *a
		}

		ev := honey.Event("search")
		ev.AddField("query", r.rawQuery())
		ev.AddField("actor_uid", act.UID)
		ev.AddField("actor_internal", act.Internal)
		ev.AddField("type", trace.GraphQLRequestName(ctx))
		ev.AddField("source", string(trace.RequestSource(ctx)))
		ev.AddField("status", status)
		ev.AddField("alert_type", alertType)
		ev.AddField("duration_ms", time.Since(start).Milliseconds())
		if rr != nil {
			ev.AddField("result_size", len(rr.SearchResults))
		}

		if honey.Enabled() {
			_ = ev.Send()
		}

		if isSlow {
			log15.Warn("slow search request", mapToLog15Ctx(ev.Fields())...)
		}
	}

	return rr, err
}

// unionMerge performs a merge of file match results, merging line matches when
// they occur in the same file, and taking care to update match counts.
func unionMerge(left, right *SearchResultsResolver) *SearchResultsResolver {
	var count int // count non-overlapping files when we merge.
	var merged []SearchResultResolver
	rightFileMatches := make(map[string]*FileMatchResolver)
	rightRepoMatches := make(map[string]*RepositoryResolver)
	rightCommitMatches := make(map[string]*CommitSearchResultResolver)
	rightDiffMatches := make(map[string]*CommitSearchResultResolver)

	// accumulate matches for the right subexpression in a lookup.
	for _, r := range right.SearchResults {
		if fileMatch, ok := r.ToFileMatch(); ok {
			rightFileMatches[fileMatch.uri] = fileMatch
			continue
		}
		if repoMatch, ok := r.ToRepository(); ok {
			rightRepoMatches[repoMatch.URL()] = repoMatch
			continue
		}
		if commitMatch, ok := r.ToCommitSearchResult(); ok {
			if commitMatch.DiffPreview() != nil {
				rightDiffMatches[commitMatch.URL()] = commitMatch
			} else {
				rightCommitMatches[commitMatch.URL()] = commitMatch
			}
			continue
		}
		merged = append(merged, r)
	}

	for _, leftMatch := range left.SearchResults {
		if leftFileMatch, ok := leftMatch.ToFileMatch(); ok {
			rightFileMatch := rightFileMatches[leftFileMatch.uri]
			if rightFileMatch == nil {
				// no overlap with existing matches.
				merged = append(merged, leftMatch)
				count++
				continue
			}
			// merge line matches with a file match that already exists.
			rightFileMatch.appendMatches(leftFileMatch)
			rightFileMatches[leftFileMatch.uri] = rightFileMatch
			continue
		}

		if leftRepoMatch, ok := leftMatch.ToRepository(); ok {
			rightRepoMatch := rightRepoMatches[string(leftRepoMatch.URL())]
			if rightRepoMatch == nil {
				// no overlap with existing matches.
				merged = append(merged, leftMatch)
				count++
			}
			continue
		}

		if leftCommitMatch, ok := leftMatch.ToCommitSearchResult(); ok {
			if leftCommitMatch.DiffPreview() != nil {
				rightDiffMatch := rightDiffMatches[leftCommitMatch.URL()]
				if rightDiffMatch == nil {
					merged = append(merged, leftCommitMatch)
					count++
				}
			} else {
				rightCommitMatch := rightCommitMatches[leftCommitMatch.URL()]
				if rightCommitMatch == nil {
					merged = append(merged, leftCommitMatch)
					count++
				}
			}
			continue
		}
		merged = append(merged, leftMatch)
	}

	for _, v := range rightFileMatches {
		merged = append(merged, v)
	}
	for _, v := range rightRepoMatches {
		merged = append(merged, v)
	}
	for _, v := range rightCommitMatches {
		merged = append(merged, v)
	}
	for _, v := range rightDiffMatches {
		merged = append(merged, v)
	}

	left.SearchResults = merged
	left.Stats.Update(&right.Stats)
	return left
}

// union returns the union of two sets of search results and merges common search data.
func union(left, right *SearchResultsResolver) *SearchResultsResolver {
	if right == nil {
		return left
	}
	if left == nil {
		return right
	}

	if left.SearchResults != nil && right.SearchResults != nil {
		return unionMerge(left, right)
	} else if right.SearchResults != nil {
		return right
	}
	return left
}

// intersectMerge performs a merge of file match results, merging line matches
// for files contained in both result sets, and updating counts.
func intersectMerge(left, right *SearchResultsResolver) *SearchResultsResolver {
	rightFileMatches := make(map[string]*FileMatchResolver)
	for _, r := range right.SearchResults {
		if fileMatch, ok := r.ToFileMatch(); ok {
			rightFileMatches[fileMatch.uri] = fileMatch
		}
	}

	var merged []SearchResultResolver
	for _, leftMatch := range left.SearchResults {
		leftFileMatch, ok := leftMatch.ToFileMatch()
		if !ok {
			continue
		}

		rightFileMatch := rightFileMatches[leftFileMatch.uri]
		if rightFileMatch == nil {
			continue
		}

		leftFileMatch.appendMatches(rightFileMatch)
		merged = append(merged, leftMatch)
	}
	left.SearchResults = merged
	left.Stats.Update(&right.Stats)
	return left
}

// intersect returns the intersection of two sets of search result content
// matches, based on whether a single file path contains content matches in both sets.
func intersect(left, right *SearchResultsResolver) *SearchResultsResolver {
	if left == nil || right == nil {
		return nil
	}
	return intersectMerge(left, right)
}

// evaluateAndStream is a wrapper around evaluateAnd which temporarily suspends
// streaming and waits for evaluateAnd to return before streaming results back on
// r.resultChannel.
func (r *searchResolver) evaluateAndStream(ctx context.Context, scopeParameters []query.Node, operands []query.Node) (*SearchResultsResolver, error) {
	// Streaming disabled.
	if r.resultChannel == nil {
		return r.evaluateAnd(ctx, scopeParameters, operands)
	}
	// For streaming search we want to run the evaluation of AND expressions in batch
	// mode. We copy r to r2 and replace the result channel with a sink.
	r2 := *r
	sink := make(chan SearchEvent)
	defer close(sink)
	go func() {
		for range sink {
		}
	}()
	r2.resultChannel = sink

	result, err := r2.evaluateAnd(ctx, scopeParameters, operands)
	r.resultChannel <- SearchEvent{
		Results: result.SearchResults,
		Stats:   result.Stats,
		Error:   err,
	}
	return result, err
}

// evaluateAnd performs set intersection on result sets. It collects results for
// all expressions that are ANDed together by searching for each subexpression
// and then intersects those results that are in the same repo/file path. To
// collect N results for count:N, we need to opportunistically ask for more than
// N results for each subexpression (since intersect can never yield more than N,
// and likely yields fewer than N results). If the intersection does not yield N
// results, and is not exhaustive for every expression, we rerun the search by
// doubling count again.
func (r *searchResolver) evaluateAnd(ctx context.Context, scopeParameters []query.Node, operands []query.Node) (*SearchResultsResolver, error) {
	start := time.Now()

	if len(operands) == 0 {
		return nil, nil
	}

	var (
		err        error
		result     *SearchResultsResolver
		termResult *SearchResultsResolver
	)

	// The number of results we want. Note that for intersect, this number
	// corresponds to documents, not line matches. By default, we ask for at
	// least 5 documents to fill the result page.
	want := 5
	// The fraction of file matches two terms share on average
	averageIntersection := 0.05
	// When we retry, cap the max search results we request for each expression
	// if search continues to not be exhaustive. Alert if exceeded.
	maxTryCount := 40000

	// Set an overall timeout in addition to the timeouts that are set for leaf-requests.
	ctx, cancel, err := r.withTimeout(ctx)
	if err != nil {
		return nil, err
	}
	defer cancel()

	// Set count: if not specified.
	var countStr string
	query.VisitField(scopeParameters, "count", func(value string, _ bool, _ query.Annotation) {
		countStr = value
	})
	if countStr != "" {
		// Override "want" if count is specified.
		want, _ = strconv.Atoi(countStr) // Invariant: count is validated.
	} else {
		scopeParameters = append(scopeParameters, query.Parameter{
			Field: "count",
			Value: strconv.FormatInt(int64(want), 10),
		})
	}

	// tryCount starts small but grows exponentially with the number of operands. It is capped at maxTryCount.
	tryCount := int(math.Floor(float64(want) / math.Pow(averageIntersection, float64(len(operands)-1))))
	if tryCount > maxTryCount {
		tryCount = maxTryCount
	}

	var exhausted bool
	for {
		scopeParameters = query.MapParameter(scopeParameters, func(field, value string, negated bool, annotation query.Annotation) query.Node {
			if field == "count" {
				value = strconv.FormatInt(int64(tryCount), 10)
			}
			return query.Parameter{Field: field, Value: value, Negated: negated, Annotation: annotation}
		})

		result, err = r.evaluatePatternExpression(ctx, scopeParameters, operands[0])
		if err != nil {
			return nil, err
		}
		if result == nil {
			return nil, nil
		}
		exhausted = !result.IsLimitHit
		for _, term := range operands[1:] {
			// check if we exceed the overall time limit before running the next query.
			select {
			case <-ctx.Done():
				usedTime := time.Since(start)
				suggestTime := longer(2, usedTime)
				return alertForTimeout(usedTime, suggestTime, r).wrap(), nil
			default:
			}

			termResult, err = r.evaluatePatternExpression(ctx, scopeParameters, term)
			if err != nil {
				return nil, err
			}
			if termResult != nil {
				exhausted = exhausted && !termResult.IsLimitHit
				result = intersect(result, termResult)
			}
		}
		if exhausted {
			break
		}
		if len(result.SearchResults) >= want {
			break
		}
		// If the result size set is not big enough, and we haven't
		// exhausted search on all expressions, double the tryCount and search more.
		tryCount *= 2
		if tryCount > maxTryCount {
			// We've capped out what we're willing to do, throw alert.
			return alertForCappedAndExpression().wrap(), nil
		}
	}
	result.IsLimitHit = !exhausted
	return result, nil
}

// evaluateOr performs set union on result sets. It collects results for all
// expressions that are ORed together by searching for each subexpression. If
// the maximum number of results are reached after evaluating a subexpression,
// we shortcircuit and return results immediately.
func (r *searchResolver) evaluateOr(ctx context.Context, scopeParameters []query.Node, operands []query.Node) (*SearchResultsResolver, error) {
	if len(operands) == 0 {
		return nil, nil
	}

	var countStr string
	wantCount := defaultMaxSearchResults
	query.VisitField(scopeParameters, "count", func(value string, _ bool, _ query.Annotation) {
		countStr = value
	})
	if countStr != "" {
		wantCount, _ = strconv.Atoi(countStr) // Invariant: count is validated.
	}

	result, err := r.evaluatePatternExpression(ctx, scopeParameters, operands[0])
	if err != nil {
		return nil, err
	}
	if result == nil {
		return nil, nil
	}
	// Do not rely on result.Stats.resultCount because it may
	// count non-content matches and there's no easy way to know.
	if len(result.SearchResults) > wantCount {
		result.SearchResults = result.SearchResults[:wantCount]
		return result, nil
	}
	var new *SearchResultsResolver
	for _, term := range operands[1:] {
		new, err = r.evaluatePatternExpression(ctx, scopeParameters, term)
		if err != nil {
			return nil, err
		}
		if new != nil {
			result = union(result, new)
			// Do not rely on result.Stats.resultCount because it may
			// count non-content matches and there's no easy way to know.
			if len(result.SearchResults) > wantCount {
				result.SearchResults = result.SearchResults[:wantCount]
				return result, nil
			}
		}
	}
	return result, nil
}

func (r *searchResolver) evaluateOperator(ctx context.Context, scopeParameters []query.Node, operator query.Operator) (*SearchResultsResolver, error) {
	if len(operator.Operands) == 0 {
		return nil, nil
	}
	var result *SearchResultsResolver
	var err error
	if operator.Kind == query.And {
		result, err = r.evaluateAndStream(ctx, scopeParameters, operator.Operands)
	} else {
		result, err = r.evaluateOr(ctx, scopeParameters, operator.Operands)
	}
	if err != nil {
		return nil, err
	}
	return result, nil
}

// setQuery sets a new query in the search resolver, for potentially repeated
// calls in the search pipeline. The important part is it takes care of
// invalidating cached repo info.
func (r *searchResolver) setQuery(q []query.Node) {
	if r.invalidateRepoCache {
		r.resolved.RepoRevs = nil
		r.resolved.MissingRepoRevs = nil
		r.repoErr = nil
	}
	r.query.(*query.AndOrQuery).Query = q
}

// evaluatePatternExpression evaluates a search pattern containing and/or expressions.
func (r *searchResolver) evaluatePatternExpression(ctx context.Context, scopeParameters []query.Node, node query.Node) (*SearchResultsResolver, error) {
	switch term := node.(type) {
	case query.Operator:
		if term.Kind == query.And || term.Kind == query.Or {
			return r.evaluateOperator(ctx, scopeParameters, term)
		} else if term.Kind == query.Concat {
			r.setQuery(append(scopeParameters, term))
			return r.evaluateLeaf(ctx)
		}
	case query.Pattern:
		r.setQuery(append(scopeParameters, term))
		return r.evaluateLeaf(ctx)
	case query.Parameter:
		// evaluatePatternExpression does not process Parameter nodes.
		return nil, nil
	}
	// Unreachable.
	return nil, fmt.Errorf("unrecognized type %T in evaluatePatternExpression", node)
}

// evaluate evaluates all expressions of a search query.
func (r *searchResolver) evaluate(ctx context.Context, q []query.Node) (*SearchResultsResolver, error) {
	scopeParameters, pattern, err := query.PartitionSearchPattern(q)
	if err != nil {
		return alertForQuery("", err).wrap(), nil
	}
	if pattern == nil {
		r.setQuery(scopeParameters)
		return r.evaluateLeaf(ctx)
	}
	result, err := r.evaluatePatternExpression(ctx, scopeParameters, pattern)
	if err != nil {
		return nil, err
	}
	r.sortResults(ctx, result.SearchResults)
	return result, nil
}

// invalidateRepoCache returns whether resolved repos should be invalidated when
// evaluating subexpressions. If a query contains more than one repo, revision,
// or repogroup field, we should invalidate resolved repos, since multiple
// repos, revisions, or repogroups imply that different repos may need to be
// resolved.
func invalidateRepoCache(q []query.Node) bool {
	var seenRepo, seenRevision, seenRepoGroup int
	query.VisitField(q, "repo", func(_ string, _ bool, _ query.Annotation) {
		seenRepo += 1
	})
	query.VisitField(q, "rev", func(_ string, _ bool, _ query.Annotation) {
		seenRevision += 1
	})
	query.VisitField(q, "repogroup", func(_ string, _ bool, _ query.Annotation) {
		seenRepoGroup += 1
	})
	return seenRepo+seenRepoGroup > 1 || seenRevision > 1
}

func (r *searchResolver) Results(ctx context.Context) (srr *SearchResultsResolver, err error) {
	tr, ctx := trace.New(ctx, "Results", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()
	switch q := r.query.(type) {
	case *query.OrdinaryQuery:
		srr, err = r.evaluateLeaf(ctx)
	case *query.AndOrQuery:
		var countStr string
		wantCount := defaultMaxSearchResults
		query.VisitField(q.Query, "count", func(value string, _ bool, _ query.Annotation) {
			countStr = value
		})
		if countStr != "" {
			wantCount, _ = strconv.Atoi(countStr) // Invariant: count is validated.
		}

		if invalidateRepoCache(q.Query) {
			r.invalidateRepoCache = true
		}
		for _, disjunct := range query.Dnf(q.Query) {
			disjunct = query.ConcatRevFilters(disjunct)
			newResult, err := r.evaluate(ctx, disjunct)
			if err != nil {
				// Fail if any subquery fails.
				return nil, err
			}
			if newResult != nil {
				srr = union(srr, newResult)
				if len(srr.SearchResults) > wantCount {
					srr.SearchResults = srr.SearchResults[:wantCount]
					break
				}

			}
		}
		if srr != nil {
			r.sortResults(ctx, srr.SearchResults)
		}
	default:
		// Unreachable.
		return nil, fmt.Errorf("unrecognized type %T in searchResolver Results", r.query)
	}
	// copy userSettings from searchResolver to SearchResultsResolver
	if srr != nil {
		srr.UserSettings = r.userSettings
	}
	if srr == nil {
		srr = &SearchResultsResolver{}
	}
	return srr, err
}

// resultsWithTimeoutSuggestion calls doResults, and in case of deadline
// exceeded returns a search alert with a did-you-mean link for the same
// query with a longer timeout.
func (r *searchResolver) resultsWithTimeoutSuggestion(ctx context.Context) (*SearchResultsResolver, error) {
	start := time.Now()
	rr, err := r.doResults(ctx, "")

	// If we encountered a context timeout, it indicates one of the many result
	// type searchers (file, diff, symbol, etc) completely timed out and could not
	// produce even partial results. Other searcher types may have produced results.
	//
	// In this case, or if we got a partial timeout where ALL repositories timed out,
	// we do not return partial results and instead display a timeout alert.
	shouldShowAlert := err == context.DeadlineExceeded
	if err == nil && rr.allReposTimedout() {
		shouldShowAlert = true
	}
	if shouldShowAlert {
		usedTime := time.Since(start)
		suggestTime := longer(2, usedTime)
		return alertForTimeout(usedTime, suggestTime, r).wrap(), nil
	}
	return rr, err
}

// longer returns a suggested longer time to wait if the given duration wasn't long enough.
func longer(N int, dt time.Duration) time.Duration {
	dt2 := func() time.Duration {
		Ndt := time.Duration(N) * dt
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

var decimalRx = lazyregexp.New(`\d+\.\d+`)

// roundStr rounds the first number containing a decimal within a string
func roundStr(s string) string {
	return decimalRx.ReplaceAllStringFunc(s, func(ns string) string {
		f, err := strconv.ParseFloat(ns, 64)
		if err != nil {
			return s
		}
		f = math.Round(f)
		return strconv.Itoa(int(f))
	})
}

type searchResultsStats struct {
	JApproximateResultCount string
	JSparkline              []int32

	sr *searchResolver

	once   sync.Once
	srs    *SearchResultsResolver
	srsErr error
}

func (srs *searchResultsStats) ApproximateResultCount() string { return srs.JApproximateResultCount }
func (srs *searchResultsStats) Sparkline() []int32             { return srs.JSparkline }

var (
	searchResultsStatsCache   = rcache.NewWithTTL("search_results_stats", 3600) // 1h
	searchResultsStatsCounter = prometheus.NewCounterVec(prometheus.CounterOpts{
		Name: "src_graphql_search_results_stats_cache_hit",
		Help: "Counts cache hits and misses for search results stats (e.g. sparklines).",
	}, []string{"type"})
)

func init() {
	prometheus.MustRegister(searchResultsStatsCounter)
}

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
		v, err = r.doResults(ctx, "")
		if err != nil {
			return nil, err // do not cache errors.
		}
		if v.MatchCount() > 0 {
			break
		}

		cloning := len(v.Cloning())
		timedout := len(v.Timedout())
		if cloning == 0 && timedout == 0 {
			break // zero results, but no cloning or timed out repos. No point in retrying.
		}

		if attempts > 5 {
			log15.Error("failed to generate sparkline due to cloning or timed out repos", "cloning", len(v.Cloning()), "timedout", len(v.Timedout()))
			return nil, fmt.Errorf("failed to generate sparkline due to %d cloning %d timedout repos", len(v.Cloning()), len(v.Timedout()))
		}

		// We didn't find any search results. Some repos are cloning or timed
		// out, so try again in a few seconds.
		attempts++
		log15.Warn("sparkline generation found 0 search results due to cloning or timed out repos (retrying in 5s)", "cloning", len(v.Cloning()), "timedout", len(v.Timedout()))
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

type getPatternInfoOptions struct {
	// forceFileSearch, when true, specifies that the search query should be
	// treated as if every default term had `file:` before it. This can be used
	// to allow users to jump to files by just typing their name.
	forceFileSearch         bool
	performStructuralSearch bool
	performLiteralSearch    bool

	fileMatchLimit int32
}

// getPatternInfo gets the search pattern info for the query in the resolver.
func (r *searchResolver) getPatternInfo(opts *getPatternInfoOptions) (*search.TextPatternInfo, error) {
	if opts == nil {
		opts = &getPatternInfoOptions{}
	}

	if opts.fileMatchLimit == 0 {
		opts.fileMatchLimit = r.maxResults()
	}

	return getPatternInfo(r.query, opts)
}

func isPatternNegated(q []query.Node) bool {
	isNegated := false
	patternsFound := 0
	query.VisitPattern(q, func(_ string, negated bool, _ query.Annotation) {
		patternsFound++
		if patternsFound > 1 {
			return
		}
		isNegated = negated
	})

	// we only support negation for queries that contain exactly 1 pattern.
	if patternsFound > 1 {
		return false
	}
	return isNegated
}

// processSearchPattern processes the search pattern for a query. It handles the interpretation of search patterns
// as literal, regex, or structural patterns, and applies fuzzy regex matching if applicable.
func processSearchPattern(q query.QueryInfo, opts *getPatternInfoOptions) (string, bool, bool, bool) {
	var pattern string
	var pieces []string
	var contentFieldSet bool
	isRegExp := false
	isStructuralPat := false

	patternValues := q.Values(query.FieldDefault)

	isNegated := false
	if andOrQuery, ok := q.(*query.AndOrQuery); ok {
		isNegated = isPatternNegated(andOrQuery.Query)
	}

	if overridePattern := q.Values(query.FieldContent); len(overridePattern) > 0 {
		patternValues = overridePattern
		contentFieldSet = true
	}

	if opts.performStructuralSearch {
		isStructuralPat = true
		for _, v := range patternValues {
			if piece := v.ToString(); piece != "" {
				pieces = append(pieces, piece)
			}
		}
		pattern = strings.Join(pieces, " ")
	} else if !opts.forceFileSearch {
		isRegExp = true
		for _, v := range patternValues {
			var piece string
			switch {
			case v.String != nil:
				if contentFieldSet && !opts.performLiteralSearch {
					piece = *v.String
				} else {
					// Treat quoted strings as literal
					// strings to match, not regexps.
					piece = regexp.QuoteMeta(*v.String)
				}
			case v.Regexp != nil:
				piece = v.Regexp.String()
			}
			if piece == "" {
				continue
			}
			pieces = append(pieces, piece)
		}
		pattern = orderedFuzzyRegexp(pieces)
	} else {
		// TODO: We must have some pattern that always matches here, or else
		// cmd/searcher/search/matcher.go:97 would cause a nil regexp panic
		// when not using indexed search. I am unsure what the right solution
		// is here. Would this code path go away when we switch fully to
		// indexed search @keegan? This workaround is OK for now though.
		isRegExp = true
		pattern = "."
	}

	return pattern, isRegExp, isStructuralPat, isNegated
}

// getPatternInfo gets the search pattern info for q
func getPatternInfo(q query.QueryInfo, opts *getPatternInfoOptions) (*search.TextPatternInfo, error) {
	pattern, isRegExp, isStructuralPat, isNegated := processSearchPattern(q, opts)

	// Handle file: and -file: filters.
	includePatterns, excludePatterns := q.RegexpPatterns(query.FieldFile)
	filePatternsReposMustInclude, filePatternsReposMustExclude := q.RegexpPatterns(query.FieldRepoHasFile)

	if opts.forceFileSearch {
		for _, v := range q.Values(query.FieldDefault) {
			includePatterns = append(includePatterns, v.ToString())
		}
	}

	var combyRule []string
	for _, v := range q.Values(query.FieldCombyRule) {
		combyRule = append(combyRule, v.ToString())
	}

	// Handle lang: and -lang: filters.
	langIncludePatterns, langExcludePatterns, err := langIncludeExcludePatterns(q.StringValues(query.FieldLang))
	if err != nil {
		return nil, err
	}
	includePatterns = append(includePatterns, langIncludePatterns...)
	excludePatterns = append(excludePatterns, langExcludePatterns...)

	languages, _ := q.StringValues(query.FieldLang)

	patternInfo := &search.TextPatternInfo{
		IsRegExp:                     isRegExp,
		IsStructuralPat:              isStructuralPat,
		IsCaseSensitive:              q.IsCaseSensitive(),
		FileMatchLimit:               opts.fileMatchLimit,
		Pattern:                      pattern,
		IsNegated:                    isNegated,
		IncludePatterns:              includePatterns,
		FilePatternsReposMustInclude: filePatternsReposMustInclude,
		FilePatternsReposMustExclude: filePatternsReposMustExclude,
		Languages:                    languages,
		PathPatternsAreCaseSensitive: q.IsCaseSensitive(),
		CombyRule:                    strings.Join(combyRule, ""),
	}
	if len(excludePatterns) > 0 {
		patternInfo.ExcludePattern = searchrepos.UnionRegExps(excludePatterns)
	}
	return patternInfo, nil
}

// langIncludeExcludePatterns returns regexps for the include/exclude path patterns given the lang:
// and -lang: filter values in a search query. For example, a query containing "lang:go" should
// include files whose paths match /\.go$/.
func langIncludeExcludePatterns(values, negatedValues []string) (includePatterns, excludePatterns []string, err error) {
	do := func(values []string, patterns *[]string) error {
		for _, value := range values {
			lang, ok := enry.GetLanguageByAlias(value)
			if !ok {
				return fmt.Errorf("unknown language: %q", value)
			}
			exts := enry.GetLanguageExtensions(lang)
			extPatterns := make([]string, len(exts))
			for i, ext := range exts {
				// Add `\.ext$` pattern to match files with the given extension.
				extPatterns[i] = regexp.QuoteMeta(ext) + "$"
			}
			*patterns = append(*patterns, searchrepos.UnionRegExps(extPatterns))
		}
		return nil
	}

	if err := do(values, &includePatterns); err != nil {
		return nil, nil, err
	}
	if err := do(negatedValues, &excludePatterns); err != nil {
		return nil, nil, err
	}
	return includePatterns, excludePatterns, nil
}

var (
	// The default timeout to use for queries.
	defaultTimeout = 20 * time.Second
)

func (r *searchResolver) searchTimeoutFieldSet() bool {
	timeout, _ := r.query.StringValue(query.FieldTimeout)
	return timeout != "" || r.countIsSet()
}

func (r *searchResolver) withTimeout(ctx context.Context) (context.Context, context.CancelFunc, error) {
	d := defaultTimeout
	maxTimeout := time.Duration(searchrepos.SearchLimits().MaxTimeoutSeconds) * time.Second
	timeout, _ := r.query.StringValue(query.FieldTimeout)
	if timeout != "" {
		var err error
		d, err = time.ParseDuration(timeout)
		if err != nil {
			return nil, nil, errors.WithMessage(err, `invalid "timeout:" value (examples: "timeout:2s", "timeout:200ms")`)
		}
	} else if r.countIsSet() {
		// If `count:` is set but `timeout:` is not explicitly set, use the max timeout
		d = maxTimeout
	}
	if d > maxTimeout {
		d = maxTimeout
	}
	ctx, cancel := context.WithTimeout(ctx, d)
	return ctx, cancel, nil
}

func (r *searchResolver) determineResultTypes(args search.TextParameters, forceOnlyResultType string) (resultTypes []string) {
	// Determine which types of results to return.
	if forceOnlyResultType != "" {
		resultTypes = []string{forceOnlyResultType}
	} else {
		resultTypes, _ = r.query.StringValues(query.FieldType)
		if len(resultTypes) == 0 {
			resultTypes = []string{"file", "path", "repo"}
		}
	}
	for _, resultType := range resultTypes {
		if resultType == "file" {
			args.PatternInfo.PatternMatchesContent = true
		} else if resultType == "path" {
			args.PatternInfo.PatternMatchesPath = true
		}
	}
	return resultTypes
}

func (r *searchResolver) determineRepos(ctx context.Context, tr *trace.Trace, start time.Time) (resolved searchrepos.Resolved, res *SearchResultsResolver, err error) {
	resolved, err = r.resolveRepositories(ctx, nil)
	if err != nil {
		if errors.Is(err, authz.ErrStalePermissions{}) {
			log15.Debug("searchResolver.determineRepos", "err", err)
			alert := alertForStalePermissions()
			return searchrepos.Resolved{}, &SearchResultsResolver{alert: alert, start: start}, nil
		}
		e := git.BadCommitError{}
		if errors.As(err, &e) {
			alert := r.alertForInvalidRevision(e.Spec)
			return searchrepos.Resolved{}, &SearchResultsResolver{alert: alert, start: start}, nil
		}
		return searchrepos.Resolved{}, nil, err
	}

	tr.LazyPrintf("searching %d repos, %d missing", len(resolved.RepoRevs), len(resolved.MissingRepoRevs))
	if len(resolved.RepoRevs) == 0 {
		alert := r.alertForNoResolvedRepos(ctx)
		return searchrepos.Resolved{}, &SearchResultsResolver{alert: alert, start: start}, nil
	}
	if resolved.OverLimit {
		alert := r.alertForOverRepoLimit(ctx)
		return searchrepos.Resolved{}, &SearchResultsResolver{alert: alert, start: start}, nil
	}
	return resolved, nil, nil
}

type DiffCommitError struct {
	ResultType string
	Max        int
}

type RepoLimitErr DiffCommitError
type TimeLimitErr DiffCommitError

func (RepoLimitErr) Error() string {
	return "repo limit error"
}

func (TimeLimitErr) Error() string {
	return "time limit error"
}

func checkDiffCommitSearchLimits(ctx context.Context, args *search.TextParameters, resultType string) error {
	repos, err := getRepos(ctx, args.RepoPromise)
	if err != nil {
		return err
	}

	hasTimeFilter := false
	if _, afterPresent := args.Query.Fields()["after"]; afterPresent {
		hasTimeFilter = true
	}
	if _, beforePresent := args.Query.Fields()["before"]; beforePresent {
		hasTimeFilter = true
	}

	limits := searchrepos.SearchLimits()
	if max := limits.CommitDiffMaxRepos; !hasTimeFilter && len(repos) > max {
		return RepoLimitErr{ResultType: resultType, Max: max}
	}
	if max := limits.CommitDiffWithTimeFilterMaxRepos; hasTimeFilter && len(repos) > max {
		return TimeLimitErr{ResultType: resultType, Max: max}
	}
	return nil
}

func newAggregator(ctx context.Context, stream SearchStream) *aggregator {
	childStream := make(chan SearchEvent, cap(stream))
	agg := &aggregator{
		stream: childStream,
		done:   make(chan struct{}),
	}

	go func() {
		defer close(agg.done)
		for event := range childStream {
			// Timeouts are reported through Stats so don't report an error for them
			if event.Error != nil && !isContextError(ctx, event.Error) {
				event.Error = nil
			}
			if event.Error != nil {
				agg.multiErr = multierror.Append(agg.multiErr, event.Error)
			}
			agg.results = append(agg.results, event.Results...)
			agg.common.Update(&event.Stats)
			if stream != nil {
				stream <- event
			}
		}
	}()

	return agg
}

type aggregator struct {
	stream SearchStream

	done chan struct{}

	results  []SearchResultResolver
	common   streaming.Stats
	multiErr *multierror.Error
}

// get finalises aggregation over the stream and returns the aggregated
// result. It should only be called once each do* function is finished
// running.
func (a *aggregator) get() ([]SearchResultResolver, streaming.Stats, *multierror.Error) {
	close(a.stream)
	<-a.done

	return a.results, a.common, a.multiErr
}

func (a *aggregator) send(event SearchEvent) {
	a.stream <- event
}

func (a *aggregator) doRepoSearch(ctx context.Context, args *search.TextParameters, limit int32) {
	tr, ctx := trace.New(ctx, "doRepoSearch", "")
	defer tr.Finish()
	results, stats, err := searchRepositories(ctx, args, limit)
	a.send(SearchEvent{
		Results: results,
		Stats:   statsDeref(stats),
		Error:   errors.Wrap(err, "repository search failed"),
	})
}

func (a *aggregator) doSymbolSearch(ctx context.Context, args *search.TextParameters, limit int) {
	tr, ctx := trace.New(ctx, "doSymbolSearch", "")
	defer tr.Finish()

	symbolFileMatches, stats, err := searchSymbols(ctx, args, limit)

	results := make([]SearchResultResolver, len(symbolFileMatches))
	for i := range symbolFileMatches {
		results[i] = symbolFileMatches[i]
	}

	a.send(SearchEvent{
		Results: results,
		Stats:   statsDeref(stats),
		Error:   errors.Wrap(err, "symbol search failed"),
	})
}

func (a *aggregator) doFilePathSearch(ctx context.Context, args *search.TextParameters) {
	tr, ctx := trace.New(ctx, "doFilePathSearch", "")

	defer tr.Finish()

	isDefaultStructuralSearch := args.PatternInfo.IsStructuralPat && args.PatternInfo.FileMatchLimit == defaultMaxSearchResults

	if !isDefaultStructuralSearch {
		searchFilesInRepos(ctx, args, a.stream)
		return
	}

	// For structural search with default limits we retry if we get no results.

	fileResults, stats, err := searchFilesInReposBatch(ctx, args)

	if len(fileResults) == 0 && err == nil {
		// No results for structural search? Automatically search again and force Zoekt
		// to resolve more potential file matches by setting a higher FileMatchLimit.
		patternCopy := *(args.PatternInfo)
		patternCopy.FileMatchLimit = 1000
		argsCopy := *args
		argsCopy.PatternInfo = &patternCopy
		args = &argsCopy

		fileResults, stats, err = searchFilesInReposBatch(ctx, args)

		if len(fileResults) == 0 {
			// Still no results? Give up.
			log15.Warn("Structural search gives up after more exhaustive attempt. Results may have been missed.")
			stats.IsLimitHit = false // Ensure we don't display "Show more".
		}
	}

	a.send(SearchEvent{
		Results: fileMatchResultsToSearchResults(fileResults),
		Stats:   stats,
		Error:   err,
	})
}

func (a *aggregator) doDiffSearch(ctx context.Context, tp *search.TextParameters) {
	err := checkDiffCommitSearchLimits(ctx, tp, "diff")
	if err != nil {
		a.send(SearchEvent{Error: err})
		return
	}

	tr, ctx := trace.New(ctx, "doDiffSearch", "")
	defer tr.Finish()

	args, err := resolveCommitParameters(ctx, tp)
	if err != nil {
		log15.Warn("doDiffSearch: error while resolving commit parameters", "error", err)
		return
	}

	searchCommitDiffsInRepos(ctx, args, a.stream)
}

func (a *aggregator) doCommitSearch(ctx context.Context, tp *search.TextParameters) {
	err := checkDiffCommitSearchLimits(ctx, tp, "commit")
	if err != nil {
		a.send(SearchEvent{Error: err})
		return
	}

	tr, ctx := trace.New(ctx, "doCommitSearch", "")
	defer tr.Finish()

	args, err := resolveCommitParameters(ctx, tp)
	if err != nil {
		log15.Warn("doCommitSearch: error while resolving commit parameters", "error", err)
		return
	}

	searchCommitLogInRepos(ctx, args, a.stream)
}

func statsDeref(s *streaming.Stats) streaming.Stats {
	if s == nil {
		return streaming.Stats{}
	}
	return *s
}

// isGlobalSearch returns true if the query does not contain repo, repogroup, or
// repohasfile filters. For structural queries and queries with version context
// isGlobalSearch always return false.
func (r *searchResolver) isGlobalSearch() bool {
	if r.patternType == query.SearchTypeStructural {
		return false
	}
	if r.versionContext != nil && *r.versionContext != "" {
		return false
	}
	return len(r.query.Values(query.FieldRepo)) == 0 && len(r.query.Values(query.FieldRepoGroup)) == 0 && len(r.query.Values(query.FieldRepoHasFile)) == 0
}

// doResults is one of the highest level search functions that handles finding results.
//
// If forceOnlyResultType is specified, only results of the given type are returned,
// regardless of what `type:` is specified in the query string.
//
// Partial results AND an error may be returned.
func (r *searchResolver) doResults(ctx context.Context, forceOnlyResultType string) (_ *SearchResultsResolver, err error) {
	tr, ctx := trace.New(ctx, "doResults", r.rawQuery())
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	start := time.Now()

	ctx, cancel, err := r.withTimeout(ctx)
	if err != nil {
		return nil, err
	}
	defer cancel()

	options := &getPatternInfoOptions{}
	if r.patternType == query.SearchTypeStructural {
		options = &getPatternInfoOptions{performStructuralSearch: true}
		forceOnlyResultType = "file"
	}
	if r.patternType == query.SearchTypeLiteral {
		options = &getPatternInfoOptions{performLiteralSearch: true}
	}
	p, err := r.getPatternInfo(options)
	if err != nil {
		return nil, err
	}

	// Fallback to literal search for searching repos and files if
	// the structural search pattern is empty.
	if r.patternType == query.SearchTypeStructural && p.Pattern == "" {
		r.patternType = query.SearchTypeLiteral
		p.IsStructuralPat = false
		forceOnlyResultType = ""
	}

	args := search.TextParameters{
		PatternInfo:     p,
		Query:           r.query,
		UseFullDeadline: r.searchTimeoutFieldSet(),
		Zoekt:           r.zoekt,
		SearcherURLs:    r.searcherURLs,
		RepoPromise:     &search.Promise{},
	}
	if err := args.PatternInfo.Validate(); err != nil {
		return nil, &badRequestError{err}
	}

	resultTypes := r.determineResultTypes(args, forceOnlyResultType)
	tr.LazyPrintf("resultTypes: %v", resultTypes)
	var (
		requiredWg      sync.WaitGroup
		optionalWg      sync.WaitGroup
		seenResultTypes = make(map[string]struct{})
	)

	waitGroup := func(required bool) *sync.WaitGroup {
		if args.UseFullDeadline {
			// When a custom timeout is specified, all searches are required and get the full timeout.
			return &requiredWg
		}
		if required {
			return &requiredWg
		}
		return &optionalWg
	}

	agg := newAggregator(ctx, r.resultChannel)

	// This ensures we properly cleanup in the case of an early return. In
	// particular we want to cancel global searches before returning early.
	hasStartedAllBackends := false
	defer func() {
		if hasStartedAllBackends {
			return
		}
		cancel()
		requiredWg.Wait()
		optionalWg.Wait()
		_, _, _ = agg.get()
	}()

	isFileOrPath := func() bool {
		for _, rt := range resultTypes {
			if rt == "file" || rt == "path" {
				return true
			}
		}
		return false
	}

	// performance optimization: call zoekt early, resolve repos concurrently, filter
	// search results with resolved repos.
	if r.isGlobalSearch() && isFileOrPath() {
		argsIndexed := args
		argsIndexed.Mode = search.ZoektGlobalSearch
		wg := waitGroup(true)
		wg.Add(1)
		goroutine.Go(func() {
			defer wg.Done()
			agg.doFilePathSearch(ctx, &argsIndexed)
		})
		// On sourcegraph.com and for unscoped queries, determineRepos returns the subset
		// of indexed default searchrepos. No need to call searcher, because
		// len(searcherRepos) will always be 0.
		if envvar.SourcegraphDotComMode() {
			args.Mode = search.NoFilePath
		} else {
			args.Mode = search.SearcherOnly
		}
	}

	resolved, alertResult, err := r.determineRepos(ctx, tr, start)
	if err != nil {
		return nil, err
	}
	if alertResult != nil {
		return alertResult, nil
	}

	// Send down our first bit of progress.
	{
		repos := make(map[api.RepoID]*types.RepoName, len(resolved.RepoRevs))
		for _, repoRev := range resolved.RepoRevs {
			repos[repoRev.Repo.ID] = repoRev.Repo
		}

		agg.send(SearchEvent{
			Stats: streaming.Stats{
				Repos:            repos,
				ExcludedForks:    resolved.ExcludedRepos.Forks,
				ExcludedArchived: resolved.ExcludedRepos.Archived,
			},
		})
	}

	// Resolve repo promise so searches waiting on it can proceed. We do this
	// after reporting the above progress to ensure we don't get search
	// results before the above reporting.
	args.RepoPromise.Resolve(resolved.RepoRevs)

	searchedFileContentsOrPaths := false
	for _, resultType := range resultTypes {
		resultType := resultType // shadow so it doesn't change in the goroutine
		if _, seen := seenResultTypes[resultType]; seen {
			continue
		}
		seenResultTypes[resultType] = struct{}{}
		switch resultType {
		case "repo":
			wg := waitGroup(true)
			wg.Add(1)
			goroutine.Go(func() {
				defer wg.Done()
				agg.doRepoSearch(ctx, &args, r.maxResults())
			})
		case "symbol":
			wg := waitGroup(len(resultTypes) == 1)
			wg.Add(1)
			goroutine.Go(func() {
				defer wg.Done()
				agg.doSymbolSearch(ctx, &args, int(r.maxResults()))
			})
		case "file", "path":
			if searchedFileContentsOrPaths || args.Mode == search.NoFilePath {
				// type:file and type:path use same searchFilesInRepos, so don't call 2x.
				continue
			}
			searchedFileContentsOrPaths = true
			wg := waitGroup(true)
			wg.Add(1)
			goroutine.Go(func() {
				defer wg.Done()
				agg.doFilePathSearch(ctx, &args)
			})
		case "diff":
			wg := waitGroup(len(resultTypes) == 1)
			wg.Add(1)
			goroutine.Go(func() {
				defer wg.Done()
				agg.doDiffSearch(ctx, &args)
			})
		case "commit":
			wg := waitGroup(len(resultTypes) == 1)
			wg.Add(1)
			goroutine.Go(func() {
				defer wg.Done()
				agg.doCommitSearch(ctx, &args)
			})
		}
	}

	hasStartedAllBackends = true

	// Wait for required searches.
	requiredWg.Wait()

	// Give optional searches some minimum budget in case required searches return quickly.
	// Cancel all remaining searches after this minimum budget.
	budget := 100 * time.Millisecond
	elapsed := time.Since(start)
	timer := time.AfterFunc(budget-elapsed, cancel)

	// Wait for remaining optional searches to finish or get cancelled.
	optionalWg.Wait()

	timer.Stop()

	// We have to call get once all waitgroups are done since it relies on
	// collecting from the streams.
	results, common, multiErr := agg.get()

	tr.LazyPrintf("results=%d %s", len(results), &common)

	var alert *searchAlert

	multiErr, newAlert := alertForDiffCommitSearch(multiErr)
	if newAlert != nil {
		alert = newAlert
	}

	multiErr, newAlert = alertForStructuralSearch(multiErr)
	if newAlert != nil {
		alert = newAlert // takes higher precedence
	}

	if len(results) == 0 && r.patternType != query.SearchTypeStructural && comby.MatchHoleRegexp.MatchString(r.originalQuery) {
		alert = alertForStructuralSearchNotSet(r.originalQuery)
	}

	if len(resolved.MissingRepoRevs) > 0 {
		alert = alertForMissingRepoRevs(r.patternType, resolved.MissingRepoRevs)
	}

	// If we have some results, only log the error instead of returning it,
	// because otherwise the client would not receive the partial results
	if len(results) > 0 && multiErr != nil {
		log15.Error("Errors during search", "error", multiErr)
		multiErr = nil
	}

	r.sortResults(ctx, results)

	resultsResolver := SearchResultsResolver{
		start:         start,
		Stats:         common,
		SearchResults: results,
		limit:         int(r.maxResults()),
		alert:         alert,
	}

	return &resultsResolver, multiErr.ErrorOrNil()
}

// isContextError returns true if ctx.Err() is not nil or if err
// is an error caused by context cancelation or timeout.
func isContextError(ctx context.Context, err error) bool {
	return ctx.Err() != nil || err == context.Canceled || err == context.DeadlineExceeded
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

// compareFileLengths sorts file paths such that they appear earlier if they
// match file: patterns in the query exactly.
func compareFileLengths(left, right string, exactFilePatterns map[string]struct{}) bool {
	_, aMatch := exactFilePatterns[path.Base(left)]
	_, bMatch := exactFilePatterns[path.Base(right)]
	if aMatch || bMatch {
		if aMatch && bMatch {
			// Prefer shorter file names (ie root files come first)
			if len(left) != len(right) {
				return len(left) < len(right)
			}
			return left < right
		}
		// Prefer exact match
		return aMatch
	}
	return left < right
}

func compareDates(left, right *time.Time) bool {
	if left == nil || right == nil {
		return left != nil // Place the value that is defined first.
	}
	return (*left).After(*right)
}

// compareSearchResults sorts repository matches, file matches, and commits.
// Repositories and filenames are sorted alphabetically. As a refinement, if any
// filename matches a value in a non-empty set exactFilePatterns, then such
// filenames are listed earlier.
//
// Commits are sorted by date. Commits are not associated with searchrepos, and
// will always list after repository or file match results, if any.
func compareSearchResults(left, right SearchResultResolver, exactFilePatterns map[string]struct{}) bool {
	sortKeys := func(result SearchResultResolver) (string, string, *time.Time) {
		switch r := result.(type) {
		case *RepositoryResolver:
			return string(r.Name()), "", nil
		case *FileMatchResolver:
			return r.Repo.Name(), r.JPath, nil
		case *CommitSearchResultResolver:
			// Commits are relatively sorted by date, and after repo
			// or path names. We use ~ as the key for repo and
			// paths,lexicographically last in ASCII.
			return "~", "~", &r.commit.commit.Author.Date
		}
		// Unreachable.
		panic("unreachable: compareSearchResults expects RepositoryResolver, FileMatchResolver, or CommitSearchResultResolver")
	}

	arepo, afile, adate := sortKeys(left)
	brepo, bfile, bdate := sortKeys(right)

	if arepo == brepo {
		if len(exactFilePatterns) == 0 {
			if afile != bfile {
				return afile < bfile
			}
			return compareDates(adate, bdate)
		}
		return compareFileLengths(afile, bfile, exactFilePatterns)
	}
	return arepo < brepo
}

func (r *searchResolver) sortResults(ctx context.Context, results []SearchResultResolver) {
	var exactPatterns map[string]struct{}
	if getBoolPtr(r.userSettings.SearchGlobbing, false) {
		exactPatterns = r.getExactFilePatterns()
	}
	sort.Slice(results, func(i, j int) bool { return compareSearchResults(results[i], results[j], exactPatterns) })
}

// getExactFilePatterns returns the set of file patterns without glob syntax.
func (r *searchResolver) getExactFilePatterns() map[string]struct{} {
	m := map[string]struct{}{}
	query.VisitField(
		r.query.(*query.AndOrQuery).Query,
		query.FieldFile,
		func(value string, negated bool, annotation query.Annotation) {
			originalValue := r.originalQuery[annotation.Range.Start.Column+len(query.FieldFile)+1 : annotation.Range.End.Column]
			if !negated && query.ContainsNoGlobSyntax(originalValue) {
				m[originalValue] = struct{}{}
			}
		})
	return m
}

// orderedFuzzyRegexp interpolate a lazy 'match everything' regexp pattern
// to achieve an ordered fuzzy regexp match.
func orderedFuzzyRegexp(pieces []string) string {
	if len(pieces) == 0 {
		return ""
	}
	if len(pieces) == 1 {
		return pieces[0]
	}
	return "(" + strings.Join(pieces, ").*?(") + ")"
}

// logSlowSearchesThreshold returns the minimum duration configured in site
// settings for logging slow searches.
func logSlowSearchesThreshold() time.Duration {
	ms := conf.Get().ObservabilityLogSlowSearches
	if ms == 0 {
		return time.Duration(math.MaxInt64)
	}
	return time.Duration(ms) * time.Millisecond
}

// mapToLog15Ctx translates a map to log15 context fields.
func mapToLog15Ctx(m map[string]interface{}) []interface{} {
	// sort so its stable
	keys := make([]string, len(m))
	i := 0
	for k := range m {
		keys[i] = k
		i++
	}
	sort.Strings(keys)
	ctx := make([]interface{}, len(m)*2)
	for i, k := range keys {
		j := i * 2
		ctx[j] = k
		ctx[j+1] = m[k]
	}
	return ctx
}
