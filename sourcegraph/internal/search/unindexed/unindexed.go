package unindexed

import (
	"context"
	"fmt"
	"time"

	"github.com/cockroachdb/errors"
	"golang.org/x/sync/errgroup"

	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/trace"

	otlog "github.com/opentracing/opentracing-go/log"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/searcher/protocol"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/endpoint"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/mutablelimiter"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	searchrepos "github.com/sourcegraph/sourcegraph/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/searcher"
	zoektutil "github.com/sourcegraph/sourcegraph/internal/search/zoekt"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

// A global limiter on number of concurrent searcher searches.
var textSearchLimiter = mutablelimiter.New(32)

var MockSearchFilesInRepos func() ([]result.Match, *streaming.Stats, error)

// SearchFilesInRepos searches a set of repos for a pattern.
func SearchFilesInRepos(ctx context.Context, zoektArgs zoektutil.IndexedSearchRequest, searcherArgs *search.SearcherParameters, notSearcherOnly bool, stream streaming.Sender) (err error) {
	if MockSearchFilesInRepos != nil {
		matches, mockStats, err := MockSearchFilesInRepos()
		stream.Send(streaming.SearchEvent{
			Results: matches,
			Stats:   mockStats.Deref(),
		})
		return err
	}

	g, ctx := errgroup.WithContext(ctx)

	if notSearcherOnly {
		// Run literal and regexp searches on indexed repositories.
		g.Go(func() error {
			return zoektArgs.Search(ctx, stream)
		})
	}

	// Concurrently run searcher for all unindexed repos regardless whether text or regexp.
	g.Go(func() error {
		return callSearcherOverRepos(ctx, searcherArgs, stream, zoektArgs.UnindexedRepos(), false)
	})

	return g.Wait()
}

// SearchFilesInRepoBatch is a convenience function around searchFilesInRepos
// which collects the results from the stream.
func SearchFilesInReposBatch(ctx context.Context, zoektArgs zoektutil.IndexedSearchRequest, searcherArgs *search.SearcherParameters, searcherOnly bool) ([]*result.FileMatch, streaming.Stats, error) {
	matches, stats, err := streaming.CollectStream(func(stream streaming.Sender) error {
		return SearchFilesInRepos(ctx, zoektArgs, searcherArgs, searcherOnly, stream)
	})

	fms, fmErr := matchesToFileMatches(matches)
	if fmErr != nil && err == nil {
		err = errors.Wrap(fmErr, "searchFilesInReposBatch failed to convert results")
	}
	return fms, stats, err
}

var mockSearchFilesInRepo func(ctx context.Context, repo types.MinimalRepo, gitserverRepo api.RepoName, rev string, info *search.TextPatternInfo, fetchTimeout time.Duration, stream streaming.Sender) (limitHit bool, err error)

func searchFilesInRepo(ctx context.Context, searcherURLs *endpoint.Map, repo types.MinimalRepo, gitserverRepo api.RepoName, rev string, index bool, info *search.TextPatternInfo, fetchTimeout time.Duration, stream streaming.Sender) (bool, error) {
	if mockSearchFilesInRepo != nil {
		return mockSearchFilesInRepo(ctx, repo, gitserverRepo, rev, info, fetchTimeout, stream)
	}

	// Do not trigger a repo-updater lookup (e.g.,
	// backend.{GitRepo,Repos.ResolveRev}) because that would slow this operation
	// down by a lot (if we're looping over many repos). This means that it'll fail if a
	// repo is not on gitserver.
	commit, err := git.ResolveRevision(ctx, gitserverRepo, rev, git.ResolveRevisionOptions{NoEnsureRevision: true})
	if err != nil {
		return false, err
	}

	shouldBeSearched, err := repoShouldBeSearched(ctx, searcherURLs, info, repo, commit, fetchTimeout)
	if err != nil {
		return false, err
	}
	if !shouldBeSearched {
		return false, err
	}

	var indexerEndpoints []string
	if info.IsStructuralPat {
		indexerEndpoints, err = search.Indexers().Map.Endpoints()
		if err != nil {
			return false, err
		}
	}

	toMatches := newToMatches(repo, commit, &rev)
	onMatches := func(searcherMatches []*protocol.FileMatch) {
		stream.Send(streaming.SearchEvent{
			Results: toMatches(searcherMatches),
		})
	}

	return searcher.Search(ctx, searcherURLs, gitserverRepo, repo.ID, rev, commit, index, info, fetchTimeout, indexerEndpoints, onMatches)
}

// newToMatches returns a closure that converts []*protocol.FileMatch to []result.Match.
func newToMatches(repo types.MinimalRepo, commit api.CommitID, rev *string) func([]*protocol.FileMatch) []result.Match {
	return func(searcherMatches []*protocol.FileMatch) []result.Match {
		matches := make([]result.Match, 0, len(searcherMatches))
		for _, fm := range searcherMatches {
			lineMatches := make([]*result.LineMatch, 0, len(fm.LineMatches))
			for _, lm := range fm.LineMatches {
				ranges := make([][2]int32, 0, len(lm.OffsetAndLengths))
				for _, ol := range lm.OffsetAndLengths {
					ranges = append(ranges, [2]int32{int32(ol[0]), int32(ol[1])})
				}
				lineMatches = append(lineMatches, &result.LineMatch{
					Preview:          lm.Preview,
					OffsetAndLengths: ranges,
					LineNumber:       int32(lm.LineNumber),
				})
			}

			matches = append(matches, &result.FileMatch{
				File: result.File{
					Path:     fm.Path,
					Repo:     repo,
					CommitID: commit,
					InputRev: rev,
				},
				LineMatches: lineMatches,
				LimitHit:    fm.LimitHit,
			})
		}
		return matches
	}
}

// repoShouldBeSearched determines whether a repository should be searched in, based on whether the repository
// fits in the subset of repositories specified in the query's `repohasfile` and `-repohasfile` flags if they exist.
func repoShouldBeSearched(ctx context.Context, searcherURLs *endpoint.Map, searchPattern *search.TextPatternInfo, repo types.MinimalRepo, commit api.CommitID, fetchTimeout time.Duration) (shouldBeSearched bool, err error) {
	shouldBeSearched = true
	flagInQuery := len(searchPattern.FilePatternsReposMustInclude) > 0
	if flagInQuery {
		shouldBeSearched, err = repoHasFilesWithNamesMatching(ctx, searcherURLs, true, searchPattern.FilePatternsReposMustInclude, repo, commit, fetchTimeout)
		if err != nil {
			return shouldBeSearched, err
		}
	}
	negFlagInQuery := len(searchPattern.FilePatternsReposMustExclude) > 0
	if negFlagInQuery {
		shouldBeSearched, err = repoHasFilesWithNamesMatching(ctx, searcherURLs, false, searchPattern.FilePatternsReposMustExclude, repo, commit, fetchTimeout)
		if err != nil {
			return shouldBeSearched, err
		}
	}
	return shouldBeSearched, nil
}

// repoHasFilesWithNamesMatching searches in a repository for matches for the patterns in the `repohasfile` or `-repohasfile` flags, and returns
// whether or not the repoShouldBeSearched in or not, based on whether matches were returned.
func repoHasFilesWithNamesMatching(ctx context.Context, searcherURLs *endpoint.Map, include bool, repoHasFileFlag []string, repo types.MinimalRepo, commit api.CommitID, fetchTimeout time.Duration) (bool, error) {
	for _, pattern := range repoHasFileFlag {
		foundMatches := false
		onMatches := func(matches []*protocol.FileMatch) {
			if len(matches) > 0 {
				foundMatches = true
			}
		}
		p := search.TextPatternInfo{IsRegExp: true, FileMatchLimit: 1, IncludePatterns: []string{pattern}, PathPatternsAreCaseSensitive: false, PatternMatchesContent: true, PatternMatchesPath: true}
		_, err := searcher.Search(ctx, searcherURLs, repo.Name, repo.ID, "", commit, false, &p, fetchTimeout, []string{}, onMatches)
		if err != nil {
			return false, err
		}
		if include && !foundMatches || !include && foundMatches {
			// repo shouldn't be searched if it does not have matches for the patterns in `repohasfile`
			// or if it has file matches for the patterns in `-repohasfile`.
			return false, nil
		}
	}

	return true, nil
}

func matchesToFileMatches(matches []result.Match) ([]*result.FileMatch, error) {
	fms := make([]*result.FileMatch, 0, len(matches))
	for _, match := range matches {
		fm, ok := match.(*result.FileMatch)
		if !ok {
			return nil, errors.Errorf("expected only file match results")
		}
		fms = append(fms, fm)
	}
	return fms, nil
}

// callSearcherOverRepos calls searcher on searcherRepos.
func callSearcherOverRepos(
	ctx context.Context,
	args *search.SearcherParameters,
	stream streaming.Sender,
	searcherRepos []*search.RepositoryRevisions,
	index bool,
) (err error) {
	tr, ctx := trace.New(ctx, "searcherOverRepos", fmt.Sprintf("query: %s", args.PatternInfo.Pattern))
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	var fetchTimeout time.Duration
	if len(searcherRepos) == 1 || args.UseFullDeadline {
		// When searching a single repo or when an explicit timeout was specified, give it the remaining deadline to fetch the archive.
		deadline, ok := ctx.Deadline()
		if ok {
			fetchTimeout = time.Until(deadline)
		} else {
			// In practice, this case should not happen because a deadline should always be set
			// but if it does happen just set a long but finite timeout.
			fetchTimeout = time.Minute
		}
	} else {
		// When searching many repos, don't wait long for any single repo to fetch.
		fetchTimeout = 500 * time.Millisecond
	}

	tr.LogFields(
		otlog.Int64("fetch_timeout_ms", fetchTimeout.Milliseconds()),
		otlog.Int64("repos_count", int64(len(searcherRepos))),
	)

	if len(searcherRepos) == 0 {
		return nil
	}

	// The number of searcher endpoints can change over time. Inform our
	// limiter of the new limit, which is a multiple of the number of
	// searchers.
	eps, err := args.SearcherURLs.Endpoints()
	if err != nil {
		return err
	}
	textSearchLimiter.SetLimit(len(eps) * 32)

	g, ctx := errgroup.WithContext(ctx)
	g.Go(func() error {
		for _, repoAllRevs := range searcherRepos {
			if len(repoAllRevs.Revs) == 0 {
				continue
			}

			revSpecs, err := repoAllRevs.ExpandedRevSpecs(ctx)
			if err != nil {
				return err
			}

			for _, rev := range revSpecs {
				limitCtx, limitDone, err := textSearchLimiter.Acquire(ctx)
				if err != nil {
					return err
				}

				// Make a new repoRev for just the operation of searching this revspec.
				repoRev := &search.RepositoryRevisions{Repo: repoAllRevs.Repo, Revs: []search.RevisionSpecifier{{RevSpec: rev}}}
				g.Go(func() error {
					ctx, done := limitCtx, limitDone
					defer done()

					repoLimitHit, err := searchFilesInRepo(ctx, args.SearcherURLs, repoRev.Repo, repoRev.GitserverRepo(), repoRev.RevSpecs()[0], index, args.PatternInfo, fetchTimeout, stream)
					if err != nil {
						tr.LogFields(otlog.String("repo", string(repoRev.Repo.Name)), otlog.Error(err), otlog.Bool("timeout", errcode.IsTimeout(err)), otlog.Bool("temporary", errcode.IsTemporary(err)))
						log15.Warn("searchFilesInRepo failed", "error", err, "repo", repoRev.Repo.Name)
					}
					// non-diff search reports timeout through err, so pass false for timedOut
					stats, err := searchrepos.HandleRepoSearchResult(repoRev, repoLimitHit, false, err)
					stream.Send(streaming.SearchEvent{
						Stats: stats,
					})
					return err
				})
			}
		}

		return nil
	})

	return g.Wait()
}

type RepoSubsetTextSearch struct {
	ZoektArgs         *search.ZoektParameters
	SearcherArgs      *search.SearcherParameters
	NotSearcherOnly   bool
	UseIndex          query.YesNoOnly
	ContainsRefGlobs  bool
	OnMissingRepoRevs zoektutil.OnMissingRepoRevs

	RepoOpts search.RepoOptions
}

func (t *RepoSubsetTextSearch) Run(ctx context.Context, db database.DB, stream streaming.Sender) error {
	repos := &searchrepos.Resolver{DB: db, Opts: t.RepoOpts}
	return repos.Paginate(ctx, nil, func(page *searchrepos.Resolved) error {
		request, ok, err := zoektutil.OnlyUnindexed(page.RepoRevs, t.ZoektArgs.Zoekt, t.UseIndex, t.ContainsRefGlobs, t.OnMissingRepoRevs)
		if err != nil {
			return err
		}

		if !ok {
			request, err = zoektutil.NewIndexedSubsetSearchRequest(ctx, page.RepoRevs, t.UseIndex, t.ZoektArgs, t.OnMissingRepoRevs)
			if err != nil {
				return err
			}
		}

		return SearchFilesInRepos(ctx, request, t.SearcherArgs, t.NotSearcherOnly, stream)
	})
}

func (*RepoSubsetTextSearch) Name() string {
	return "RepoSubsetText"
}

type RepoUniverseTextSearch struct {
	GlobalZoektQuery *zoektutil.GlobalZoektQuery
	ZoektArgs        *search.ZoektParameters

	RepoOptions search.RepoOptions
	UserID      int32
}

func (t *RepoUniverseTextSearch) Run(ctx context.Context, db database.DB, stream streaming.Sender) error {
	userID := int32(0)

	if envvar.SourcegraphDotComMode() {
		if a := actor.FromContext(ctx); a != nil {
			userID = a.UID
		}
	}

	userPrivateRepos := searchrepos.PrivateReposForUser(ctx, db, userID, t.RepoOptions)
	t.GlobalZoektQuery.ApplyPrivateFilter(userPrivateRepos)
	t.ZoektArgs.Query = t.GlobalZoektQuery.Generate()

	g, ctx := errgroup.WithContext(ctx)
	g.Go(func() error {
		return zoektutil.DoZoektSearchGlobal(ctx, t.ZoektArgs, stream)
	})
	return g.Wait()
}

func (*RepoUniverseTextSearch) Name() string {
	return "RepoUniverseText"
}
