package graphqlbackend

import (
	"context"
	"fmt"
	"net/url"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/google/zoekt"
	zoektquery "github.com/google/zoekt/query"
	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"

	searchrepos "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/gituri"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	zoektutil "github.com/sourcegraph/sourcegraph/internal/search/zoekt"
	"github.com/sourcegraph/sourcegraph/internal/symbols/protocol"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type indexedRequestType string

const (
	textRequest   indexedRequestType = "text"
	symbolRequest indexedRequestType = "symbol"
	fileRequest   indexedRequestType = "file"
)

// indexedSearchRequest is responsible for translating a Sourcegraph search
// query into a Zoekt query and mapping the results from zoekt back to
// Sourcegraph result types.
type indexedSearchRequest struct {
	// Unindexed is a slice of repository revisions that can't be searched by
	// Zoekt. The repository revisions should be searched by the searcher
	// service.
	//
	// If IndexUnavailable is true or the query specifies index:no then all
	// repository revisions will be listed. Otherwise it will just be
	// repository revisions not indexed.
	Unindexed []*search.RepositoryRevisions

	// IndexUnavailable is true if zoekt is offline or disabled.
	IndexUnavailable bool

	// DisableUnindexedSearch is true if the query specified that only index
	// search should be used.
	DisableUnindexedSearch bool

	// inputs
	args *search.TextParameters
	typ  indexedRequestType

	// repos is the repository revisions that are indexed and will be
	// searched.
	repos *indexedRepoRevs

	// since if non-nil will be used instead of time.Since. For tests
	since func(time.Time) time.Duration
}

// TODO (stefan) move this out of zoekt.go to the new parser once it is guaranteed that the old parser is turned off for all customers
func containsRefGlobs(q query.QueryInfo) bool {
	containsRefGlobs := false
	if repoFilterValues, _ := q.RegexpPatterns(query.FieldRepo); len(repoFilterValues) > 0 {
		for _, v := range repoFilterValues {
			repoRev := strings.SplitN(v, "@", 2)
			if len(repoRev) == 1 { // no revision
				continue
			}
			if query.ContainsNoGlobSyntax(repoRev[1]) {
				continue
			}
			containsRefGlobs = true
			break
		}
	}
	return containsRefGlobs
}

func newIndexedSearchRequest(ctx context.Context, args *search.TextParameters, typ indexedRequestType) (_ *indexedSearchRequest, err error) {
	tr, ctx := trace.New(ctx, "newIndexedSearchRequest", string(typ))
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()
	repos, err := getRepos(ctx, args.RepoPromise)
	if err != nil {
		return nil, err
	}

	// Parse index:yes (default), index:only, and index:no in search query.
	indexParam := searchrepos.Yes
	if index, _ := args.Query.StringValues(query.FieldIndex); len(index) > 0 {
		index := index[len(index)-1]
		indexParam = searchrepos.ParseYesNoOnly(index)
		if indexParam == searchrepos.Invalid {
			return nil, fmt.Errorf("invalid index:%q (valid values are: yes, only, no)", index)
		}
	}

	// If Zoekt is disabled just fallback to Unindexed.
	if !args.Zoekt.Enabled() {
		if indexParam == searchrepos.Only {
			return nil, fmt.Errorf("invalid index:%q (indexed search is not enabled)", indexParam)
		}

		return &indexedSearchRequest{
			Unindexed:        repos,
			IndexUnavailable: true,
		}, nil
	}

	// Fallback to Unindexed if the query contains ref-globs
	if containsRefGlobs(args.Query) {
		if indexParam == searchrepos.Only {
			return nil, fmt.Errorf("invalid index:%q (revsions with glob pattern cannot be resolved for indexed searches)", indexParam)
		}
		return &indexedSearchRequest{
			Unindexed: repos,
		}, nil
	}

	// Fallback to Unindexed if index:no
	if indexParam == searchrepos.No {
		return &indexedSearchRequest{
			Unindexed: repos,
		}, nil
	}

	// Only include indexes with symbol information if a symbol request.
	var filter func(repo *zoekt.Repository) bool
	if typ == symbolRequest {
		filter = func(repo *zoekt.Repository) bool {
			return repo.HasSymbols
		}
	}

	// Consult Zoekt to find out which repository revisions can be searched.
	ctx, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	indexedSet, err := args.Zoekt.ListAll(ctx)
	if err != nil {
		if ctx.Err() == nil {
			// Only hard fail if the user specified index:only
			if indexParam == searchrepos.Only {
				return nil, errors.New("index:only failed since indexed search is not available yet")
			}

			log15.Warn("zoektIndexedRepos failed", "error", err)
		}

		return &indexedSearchRequest{
			Unindexed:        repos,
			IndexUnavailable: true,
		}, ctx.Err()
	}

	tr.LogFields(log.Int("all_indexed_set.size", len(indexedSet)))

	// Split based on indexed vs unindexed
	indexed, searcherRepos := zoektIndexedRepos(indexedSet, repos, filter)

	tr.LogFields(
		log.Int("indexed.size", len(indexed.repoRevs)),
		log.Int("searcher_repos.size", len(searcherRepos)),
	)

	// We do not yet support searching non-HEAD for fileRequest (structural
	// search).
	if typ == fileRequest && indexed.NotHEADOnlySearch {
		return nil, errors.New("structural search only supports searching the default branch https://github.com/sourcegraph/sourcegraph/issues/11906")
	}

	return &indexedSearchRequest{
		args: args,
		typ:  typ,

		Unindexed: searcherRepos,
		repos:     indexed,

		DisableUnindexedSearch: indexParam == searchrepos.Only,
	}, nil
}

// Repos is a map of repository revisions that are indexed and will be
// searched by Zoekt. Do not mutate.
func (s *indexedSearchRequest) Repos() map[string]*search.RepositoryRevisions {
	if s.repos == nil {
		return nil
	}
	return s.repos.repoRevs
}

type streamFunc func(context.Context, *search.TextParameters, *indexedRepoRevs, indexedRequestType, func(t time.Time) time.Duration) <-chan zoektSearchStreamEvent

type indexedSearchEvent struct {
	common  searchResultsCommon
	results []*FileMatchResolver
	err     error
}

// Search returns a search event stream. Ensure you drain the stream.
func (s *indexedSearchRequest) Search(ctx context.Context) <-chan indexedSearchEvent {
	c := make(chan indexedSearchEvent)
	go func() {
		defer close(c)
		s.doSearch(ctx, c)
	}()

	return c
}

func (s *indexedSearchRequest) doSearch(ctx context.Context, c chan<- indexedSearchEvent) {
	if s.args == nil {
		return
	}
	if len(s.Repos()) == 0 && s.args.Mode != search.ZoektGlobalSearch {
		return
	}

	since := time.Since
	if s.since != nil {
		since = s.since
	}

	var zoektStream streamFunc
	switch s.typ {
	case textRequest, symbolRequest:
		zoektStream = zoektSearchStream
	case fileRequest:
		zoektStream = zoektSearchHEADOnlyFilesStream
	default:
		err := fmt.Errorf("unexpected indexedSearchRequest type: %q", s.typ)
		c <- indexedSearchEvent{common: searchResultsCommon{}, results: nil, err: err}
		return
	}

	ctx, cancel := context.WithCancel(ctx)

	// Ensure we drain events if we return early.
	events := zoektStream(ctx, s.args, s.repos, s.typ, since)
	defer func() {
		cancel()
		for range events {
		}
	}()

	mkStatusMap := func(mask search.RepoStatus) search.RepoStatusMap {
		var statusMap search.RepoStatusMap
		for _, r := range s.Repos() {
			statusMap.Update(r.Repo.ID, mask)
		}
		return statusMap
	}

	for event := range events {
		err := event.err

		if err == errNoResultsInTimeout {
			err = nil
			c <- indexedSearchEvent{common: searchResultsCommon{status: mkStatusMap(search.RepoStatusTimedout | search.RepoStatusIndexed)}}
			return
		}

		if err != nil {
			c <- indexedSearchEvent{common: searchResultsCommon{}, results: nil, err: err}
			return
		}

		// We know that the repo for each result was searched, so include that
		// in the statusMap.
		var statusMap search.RepoStatusMap
		lastID := api.RepoID(-1) // PERF: avoid Update call if we have the same repository
		for _, fm := range event.fm {
			if id := fm.Repo.innerRepo.ID; lastID != id {
				statusMap.Update(id, search.RepoStatusSearched|search.RepoStatusIndexed)
				lastID = id
			}
		}

		// Partial is populated with repositories we may have not fully
		// searched due to limits.
		for r := range event.partial {
			statusMap.Update(r, search.RepoStatusLimitHit)
		}

		c <- indexedSearchEvent{
			common: searchResultsCommon{
				status:   statusMap,
				limitHit: event.limitHit,
			},
			results: event.fm,
			err:     nil,
		}
	}

	// Successfully searched everything. Communicate every indexed repo was
	// searched in case it didn't have a result.
	c <- indexedSearchEvent{common: searchResultsCommon{status: mkStatusMap(search.RepoStatusSearched | search.RepoStatusIndexed)}}
}

func zoektResultCountFactor(numRepos int, fileMatchLimit int32, globalSearch bool) (k int) {
	if globalSearch {
		// for globalSearch, numRepos = 0, but effectively we are searching over all
		// indexed repos, hence k should be 1
		k = 1
	} else {
		// If we're only searching a small number of repositories, return more
		// comprehensive results. This is arbitrary.
		switch {
		case numRepos <= 5:
			k = 100
		case numRepos <= 10:
			k = 10
		case numRepos <= 25:
			k = 8
		case numRepos <= 50:
			k = 5
		case numRepos <= 100:
			k = 3
		case numRepos <= 500:
			k = 2
		default:
			k = 1
		}
	}
	if fileMatchLimit > defaultMaxSearchResults {
		k = int(float64(k) * 3 * float64(fileMatchLimit) / float64(defaultMaxSearchResults))
	}
	return k
}

func getSpanContext(ctx context.Context) (shouldTrace bool, spanContext map[string]string) {
	if !ot.ShouldTrace(ctx) {
		return false, nil
	}

	spanContext = make(map[string]string)
	if err := ot.GetTracer(ctx).Inject(opentracing.SpanFromContext(ctx).Context(), opentracing.TextMap, opentracing.TextMapCarrier(spanContext)); err != nil {
		log15.Warn("Error injecting span context into map: %s", err)
		return true, nil
	}
	return true, spanContext
}

func zoektSearchOpts(ctx context.Context, k int, query *search.TextPatternInfo) zoekt.SearchOptions {
	shouldTrace, spanContext := getSpanContext(ctx)
	searchOpts := zoekt.SearchOptions{
		Trace:                  shouldTrace,
		SpanContext:            spanContext,
		MaxWallTime:            defaultTimeout,
		ShardMaxMatchCount:     100 * k,
		TotalMaxMatchCount:     100 * k,
		ShardMaxImportantMatch: 15 * k,
		TotalMaxImportantMatch: 25 * k,
		MaxDocDisplayCount:     2 * defaultMaxSearchResults,
	}

	// We want zoekt to return more than FileMatchLimit results since we use
	// the extra results to populate reposLimitHit. Additionally the defaults
	// are very low, so we always want to return at least 2000.
	if query.FileMatchLimit > defaultMaxSearchResults {
		searchOpts.MaxDocDisplayCount = 2 * int(query.FileMatchLimit)
	}
	if searchOpts.MaxDocDisplayCount < 2000 {
		searchOpts.MaxDocDisplayCount = 2000
	}

	if userProbablyWantsToWaitLonger := query.FileMatchLimit > defaultMaxSearchResults; userProbablyWantsToWaitLonger {
		searchOpts.MaxWallTime *= time.Duration(3 * float64(query.FileMatchLimit) / float64(defaultMaxSearchResults))
	}

	return searchOpts
}

var errNoResultsInTimeout = errors.New("no results found in specified timeout")

type zoektSearchStreamEvent struct {
	fm       []*FileMatchResolver
	limitHit bool
	partial  map[api.RepoID]struct{}
	err      error
}

func zoektSearchStream(ctx context.Context, args *search.TextParameters, repos *indexedRepoRevs, typ indexedRequestType, since func(t time.Time) time.Duration) <-chan zoektSearchStreamEvent {
	c := make(chan zoektSearchStreamEvent)
	go func() {
		defer close(c)
		_, _, _, _ = zoektSearch(ctx, args, repos, typ, since, c)
	}()

	return c
}

// zoektSearch searches repositories using zoekt.
//
// Timeouts are reported through the context, and as a special case errNoResultsInTimeout
// is returned if no results are found in the given timeout (instead of the more common
// case of finding partial or full results in the given timeout).
func zoektSearch(ctx context.Context, args *search.TextParameters, repos *indexedRepoRevs, typ indexedRequestType, since func(t time.Time) time.Duration, c chan<- zoektSearchStreamEvent) (fm []*FileMatchResolver, limitHit bool, partial map[api.RepoID]struct{}, err error) {
	defer func() {
		if c != nil {
			c <- zoektSearchStreamEvent{
				fm:       fm,
				limitHit: limitHit,
				partial:  partial,
				err:      err,
			}
		}
	}()

	if args == nil {
		return nil, false, nil, nil
	}
	if len(repos.repoRevs) == 0 && args.Mode != search.ZoektGlobalSearch {
		return nil, false, nil, nil
	}

	queryExceptRepos, err := queryToZoektQuery(args.PatternInfo, typ)
	if err != nil {
		return nil, false, nil, err
	}
	// Performance optimization: For queries without repo: filters, it is not
	// necessary to send the list of all repoBranches to zoekt. Zoekt can simply
	// search all its shards and we filter the results later against the list of
	// repos we resolve concurrently.
	var finalQuery zoektquery.Q
	if args.Mode == search.ZoektGlobalSearch {
		finalQuery = zoektquery.NewAnd(&zoektquery.Branch{Pattern: "HEAD", Exact: true}, queryExceptRepos)
	} else {
		finalQuery = zoektquery.NewAnd(&zoektquery.RepoBranches{Set: repos.repoBranches}, queryExceptRepos)
	}

	k := zoektResultCountFactor(len(repos.repoBranches), args.PatternInfo.FileMatchLimit, args.Mode == search.ZoektGlobalSearch)
	searchOpts := zoektSearchOpts(ctx, k, args.PatternInfo)

	if deadline, ok := ctx.Deadline(); ok {
		// If the user manually specified a timeout, allow zoekt to use all of the remaining timeout.
		searchOpts.MaxWallTime = time.Until(deadline)

		// We don't want our context's deadline to cut off zoekt so that we can get the results
		// found before the deadline.
		//
		// We'll create a new context that gets cancelled if the other context is cancelled for any
		// reason other than the deadline being exceeded. This essentially means the deadline for the new context
		// will be `deadline + time for zoekt to cancel + network latency`.
		var cancel context.CancelFunc
		ctx, cancel = contextWithoutDeadline(ctx)
		defer cancel()
	}

	t0 := time.Now()
	resp, err := args.Zoekt.Client.Search(ctx, finalQuery, &searchOpts)
	if err != nil {
		return nil, false, nil, err
	}
	if resp.FileCount == 0 && resp.MatchCount == 0 && since(t0) >= searchOpts.MaxWallTime {
		return nil, false, nil, errNoResultsInTimeout
	}
	limitHit = resp.FilesSkipped+resp.ShardsSkipped > 0

	if len(resp.Files) == 0 {
		return nil, false, nil, nil
	}

	maxLineMatches := 25 + k
	maxLineFragmentMatches := 3 + k

	var getRepoInputRev func(file *zoekt.FileMatch) (repo *types.RepoName, revs []string, ok bool)

	if args.Mode == search.ZoektGlobalSearch {
		m := map[string]*search.RepositoryRevisions{}
		for _, file := range resp.Files {
			m[file.Repository] = nil
		}
		repos, err := getRepos(ctx, args.RepoPromise)
		if err != nil {
			return nil, false, nil, err
		}

		for _, repo := range repos {
			if _, ok := m[string(repo.Repo.Name)]; !ok {
				continue
			}
			m[string(repo.Repo.Name)] = repo
		}
		getRepoInputRev = func(file *zoekt.FileMatch) (repo *types.RepoName, revs []string, ok bool) {
			repoRev := m[file.Repository]
			if repoRev == nil {
				return nil, nil, false
			}
			return repoRev.Repo, repoRev.RevSpecs(), true
		}
	} else {
		getRepoInputRev = func(file *zoekt.FileMatch) (repo *types.RepoName, revs []string, ok bool) {
			repo, inputRevs := repos.GetRepoInputRev(file)
			return repo, inputRevs, true
		}
	}

	limitHit, files, partial := zoektLimitMatches(limitHit, int(args.PatternInfo.FileMatchLimit), resp.Files, getRepoInputRev)
	resp.Files = files

	matches := make([]*FileMatchResolver, 0, len(resp.Files))
	repoResolvers := make(RepositoryResolverCache)
	for _, file := range resp.Files {
		fileLimitHit := false
		if len(file.LineMatches) > maxLineMatches {
			file.LineMatches = file.LineMatches[:maxLineMatches]
			fileLimitHit = true
			limitHit = true
		}
		repo, inputRevs, ok := getRepoInputRev(&file)
		if !ok {
			continue
		}
		repoResolver := repoResolvers[repo.Name]
		if repoResolver == nil {
			repoResolver = &RepositoryResolver{innerRepo: repo.ToRepo()}
			repoResolvers[repo.Name] = repoResolver
		}

		var lines []*lineMatch
		var matchCount int
		if typ != symbolRequest {
			lines, matchCount = zoektFileMatchToLineMatches(maxLineFragmentMatches, &file)
		}

		for _, inputRev := range inputRevs {
			inputRev := inputRev // copy so we can take the pointer

			var symbols []*searchSymbolResult
			if typ == symbolRequest {
				symbols = zoektFileMatchToSymbolResults(repoResolver, inputRev, &file)
			}

			matches = append(matches, &FileMatchResolver{
				JPath:        file.FileName,
				JLineMatches: lines,
				JLimitHit:    fileLimitHit,
				MatchCount:   matchCount, // We do not use resp.MatchCount because it counts the number of lines matched, not the number of fragments.
				uri:          fileMatchURI(repo.Name, inputRev, file.FileName),
				symbols:      symbols,
				Repo:         repoResolver,
				CommitID:     api.CommitID(file.Version),
				InputRev:     &inputRev,
			})
		}
	}

	return matches, limitHit, partial, nil
}

// zoektLimitMatches is the logic which limits files based on
// limit. Additionally it calculates the set of repos with partial
// results. This information is not returned by zoekt, so if zoekt indicates a
// limit has been hit, we include all repos in partial.
func zoektLimitMatches(limitHit bool, limit int, files []zoekt.FileMatch, getRepoInputRev func(file *zoekt.FileMatch) (repo *types.RepoName, revs []string, ok bool)) (bool, []zoekt.FileMatch, map[api.RepoID]struct{}) {
	var resultFiles []zoekt.FileMatch
	var partialFiles []zoekt.FileMatch

	resultFiles = files
	if limitHit {
		partialFiles = files
	}

	if len(files) > limit {
		resultFiles = files[:limit]
		if !limitHit {
			limitHit = true
			partialFiles = files[limit:]
		}
	}

	if len(partialFiles) == 0 {
		return limitHit, resultFiles, nil
	}

	partial := make(map[api.RepoID]struct{})
	last := ""
	for _, file := range partialFiles {
		// PERF: skip lookup if it is the same repo as the last result
		if file.Repository == last {
			continue
		}
		last = file.Repository

		if repo, _, ok := getRepoInputRev(&file); ok {
			partial[repo.ID] = struct{}{}
		}
	}

	return limitHit, resultFiles, partial
}

func zoektFileMatchToLineMatches(maxLineFragmentMatches int, file *zoekt.FileMatch) ([]*lineMatch, int) {
	var matchCount int
	lines := make([]*lineMatch, 0, len(file.LineMatches))

	for _, l := range file.LineMatches {
		if l.FileName {
			continue
		}

		if len(l.LineFragments) > maxLineFragmentMatches {
			l.LineFragments = l.LineFragments[:maxLineFragmentMatches]
		}
		offsets := make([][2]int32, len(l.LineFragments))
		for k, m := range l.LineFragments {
			offset := utf8.RuneCount(l.Line[:m.LineOffset])
			length := utf8.RuneCount(l.Line[m.LineOffset : m.LineOffset+m.MatchLength])
			offsets[k] = [2]int32{int32(offset), int32(length)}
		}
		matchCount += len(offsets)
		lines = append(lines, &lineMatch{
			JPreview:          string(l.Line),
			JLineNumber:       int32(l.LineNumber - 1),
			JOffsetAndLengths: offsets,
		})
	}

	return lines, matchCount
}

func zoektFileMatchToSymbolResults(repo *RepositoryResolver, inputRev string, file *zoekt.FileMatch) []*searchSymbolResult {
	// Symbol search returns a resolver so we need to pass in some
	// extra stuff. This is a sign that we can probably restructure
	// resolvers to avoid this.
	baseURI := &gituri.URI{URL: url.URL{Scheme: "git", Host: repo.Name(), RawQuery: url.QueryEscape(inputRev)}}
	commit := &GitCommitResolver{
		repoResolver: repo,
		oid:          GitObjectID(file.Version),
		inputRev:     &inputRev,
	}
	lang := strings.ToLower(file.Language)

	symbols := make([]*searchSymbolResult, 0, len(file.LineMatches))
	for _, l := range file.LineMatches {
		if l.FileName {
			continue
		}

		for _, m := range l.LineFragments {
			if m.SymbolInfo == nil {
				continue
			}

			symbols = append(symbols, &searchSymbolResult{
				symbol: protocol.Symbol{
					Name:       m.SymbolInfo.Sym,
					Kind:       m.SymbolInfo.Kind,
					Parent:     m.SymbolInfo.Parent,
					ParentKind: m.SymbolInfo.ParentKind,
					Path:       file.FileName,
					Line:       l.LineNumber,
				},
				lang:    lang,
				baseURI: baseURI,
				commit:  commit,
			})
		}
	}

	return symbols
}

// contextWithoutDeadline returns a context which will cancel if the cOld is
// canceled.
func contextWithoutDeadline(cOld context.Context) (context.Context, context.CancelFunc) {
	cNew, cancel := context.WithCancel(context.Background())

	// Set trace context so we still get spans propagated
	cNew = trace.CopyContext(cNew, cOld)

	go func() {
		select {
		case <-cOld.Done():
			// cancel the new context if the old one is done for some reason other than the deadline passing.
			if cOld.Err() != context.DeadlineExceeded {
				cancel()
			}
		case <-cNew.Done():
		}
	}()

	return cNew, cancel
}

func queryToZoektQuery(query *search.TextPatternInfo, typ indexedRequestType) (zoektquery.Q, error) {
	var and []zoektquery.Q

	var q zoektquery.Q
	var err error
	if query.IsRegExp {
		fileNameOnly := query.PatternMatchesPath && !query.PatternMatchesContent
		contentOnly := !query.PatternMatchesPath && query.PatternMatchesContent
		q, err = zoektutil.ParseRe(query.Pattern, fileNameOnly, contentOnly, query.IsCaseSensitive)
		if err != nil {
			return nil, err
		}
	} else {
		q = &zoektquery.Substring{
			Pattern:       query.Pattern,
			CaseSensitive: query.IsCaseSensitive,

			FileName: true,
			Content:  true,
		}
	}

	if query.IsNegated {
		q = &zoektquery.Not{Child: q}
	}

	if typ == symbolRequest {
		// Tell zoekt q must match on symbols
		q = &zoektquery.Symbol{
			Expr: q,
		}
	}

	and = append(and, q)

	// zoekt also uses regular expressions for file paths
	// TODO PathPatternsAreCaseSensitive
	// TODO whitespace in file path patterns?
	for _, p := range query.IncludePatterns {
		q, err := zoektutil.FileRe(p, query.IsCaseSensitive)
		if err != nil {
			return nil, err
		}
		and = append(and, q)
	}
	if query.ExcludePattern != "" {
		q, err := zoektutil.FileRe(query.ExcludePattern, query.IsCaseSensitive)
		if err != nil {
			return nil, err
		}
		and = append(and, &zoektquery.Not{Child: q})
	}

	// For conditionals that happen on a repo we can use type:repo queries. eg
	// (type:repo file:foo) (type:repo file:bar) will match all repos which
	// contain a filename matching "foo" and a filename matchinb "bar".
	//
	// Note: (type:repo file:foo file:bar) will only find repos with a
	// filename containing both "foo" and "bar".
	for _, p := range query.FilePatternsReposMustInclude {
		q, err := zoektutil.FileRe(p, query.IsCaseSensitive)
		if err != nil {
			return nil, err
		}
		and = append(and, &zoektquery.Type{Type: zoektquery.TypeRepo, Child: q})
	}
	for _, p := range query.FilePatternsReposMustExclude {
		q, err := zoektutil.FileRe(p, query.IsCaseSensitive)
		if err != nil {
			return nil, err
		}
		and = append(and, &zoektquery.Not{Child: &zoektquery.Type{Type: zoektquery.TypeRepo, Child: q}})
	}

	return zoektquery.Simplify(zoektquery.NewAnd(and...)), nil
}

// zoektIndexedRepos splits the revs into two parts: (1) the repository
// revisions in indexedSet (indexed) and (2) the repositories that are
// unindexed.
func zoektIndexedRepos(indexedSet map[string]*zoekt.Repository, revs []*search.RepositoryRevisions, filter func(*zoekt.Repository) bool) (indexed *indexedRepoRevs, unindexed []*search.RepositoryRevisions) {
	// PERF: If len(revs) is large, we expect to be doing an indexed
	// search. So set indexed to the max size it can be to avoid growing.
	indexed = &indexedRepoRevs{
		repoRevs:     make(map[string]*search.RepositoryRevisions, len(revs)),
		repoBranches: make(map[string][]string, len(revs)),
	}
	unindexed = make([]*search.RepositoryRevisions, 0)

	for _, reporev := range revs {
		repo, ok := indexedSet[string(reporev.Repo.Name)]
		if !ok || (filter != nil && !filter(repo)) {
			unindexed = append(unindexed, reporev)
			continue
		}

		unindexedRevs := indexed.Add(reporev, repo)
		if len(unindexedRevs) > 0 {
			copy := *reporev
			copy.Revs = unindexedRevs
			unindexed = append(unindexed, &copy)
		}
	}

	return indexed, unindexed
}

// indexedRepoRevs creates both the Sourcegraph and Zoekt representation of a
// list of repository and refs to search.
type indexedRepoRevs struct {
	// repoRevs is the Sourcegraph representation of a the list of repoRevs
	// repository and revisions to search.
	repoRevs map[string]*search.RepositoryRevisions

	// repoBranches will be used when we query zoekt. The order of branches
	// must match that in a reporev such that we can map back results. IE this
	// invariant is maintained:
	//
	//  repoBranches[reporev.Repo.Name][i] <-> reporev.Revs[i]
	repoBranches map[string][]string

	// NotHEADOnlySearch is true if we are searching a branch other than HEAD.
	//
	// This option can be removed once structural search supports searching
	// more than HEAD.
	NotHEADOnlySearch bool
}

// headBranch is used as a singleton of the indexedRepoRevs.repoBranches to save
// common-case allocations within indexedRepoRevs.Add.
var headBranch = []string{"HEAD"}

// Add will add reporev and repo to the list of repository and branches to
// search if reporev's refs are a subset of repo's branches. It will return
// the revision specifiers it can't add.
func (rb *indexedRepoRevs) Add(reporev *search.RepositoryRevisions, repo *zoekt.Repository) []search.RevisionSpecifier {
	// A repo should only appear once in revs. However, in case this
	// invariant is broken we will treat later revs as if it isn't
	// indexed.
	if _, ok := rb.repoBranches[string(reporev.Repo.Name)]; ok {
		return reporev.Revs
	}

	if !reporev.OnlyExplicit() {
		// Contains a RefGlob or ExcludeRefGlob so we can't do indexed
		// search on it.
		//
		// TODO we could only process the explicit revs and return the non
		// explicit ones as unindexed.
		return reporev.Revs
	}

	if len(reporev.Revs) == 1 && repo.Branches[0].Name == "HEAD" && (reporev.Revs[0].RevSpec == "" || reporev.Revs[0].RevSpec == "HEAD") {
		rb.repoRevs[string(reporev.Repo.Name)] = reporev
		rb.repoBranches[string(reporev.Repo.Name)] = headBranch
		return nil
	}

	// notHEADOnlySearch is set to true if we search any branch other than
	// repo.Branches[0]
	notHEADOnlySearch := false

	// Assume for large searches they will mostly involve indexed
	// revisions, so just allocate that.
	var unindexed []search.RevisionSpecifier
	indexed := make([]search.RevisionSpecifier, 0, len(reporev.Revs))

	branches := make([]string, 0, len(reporev.Revs))
	for _, rev := range reporev.Revs {
		if rev.RevSpec == "" || rev.RevSpec == "HEAD" {
			// Zoekt convention that first branch is HEAD
			branches = append(branches, repo.Branches[0].Name)
			indexed = append(indexed, rev)
			continue
		}

		found := false
		for i, branch := range repo.Branches {
			if branch.Name == rev.RevSpec {
				branches = append(branches, branch.Name)
				notHEADOnlySearch = notHEADOnlySearch || i > 0
				found = true
				break
			}
			// Check if rev is an abbrev commit SHA
			if len(rev.RevSpec) >= 4 && strings.HasPrefix(branch.Version, rev.RevSpec) {
				branches = append(branches, branch.Name)
				notHEADOnlySearch = notHEADOnlySearch || i > 0
				found = true
				break
			}
		}

		if found {
			indexed = append(indexed, rev)
		} else {
			unindexed = append(unindexed, rev)
		}
	}

	// We found indexed branches! Track them.
	if len(indexed) > 0 {
		rb.repoRevs[string(reporev.Repo.Name)] = reporev
		rb.repoBranches[string(reporev.Repo.Name)] = branches
		rb.NotHEADOnlySearch = rb.NotHEADOnlySearch || notHEADOnlySearch
	}

	return unindexed
}

// GetRepoInputRev returns the repo and inputRev associated with file.
func (rb *indexedRepoRevs) GetRepoInputRev(file *zoekt.FileMatch) (repo *types.RepoName, inputRevs []string) {
	repoRev := rb.repoRevs[file.Repository]

	inputRevs = make([]string, 0, len(file.Branches))
	for _, branch := range file.Branches {
		for i, b := range rb.repoBranches[file.Repository] {
			if branch == b {
				// RevSpec is guaranteed to be explicit via zoektIndexedRepos
				inputRevs = append(inputRevs, repoRev.Revs[i].RevSpec)
			}
		}
	}

	if len(inputRevs) == 0 {
		// Did not find a match. This is unexpected, but we can fallback to
		// file.Version to generate correct links.
		inputRevs = append(inputRevs, file.Version)
	}

	return repoRev.Repo, inputRevs
}
