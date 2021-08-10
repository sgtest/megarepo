package unindexed

import (
	"context"

	"github.com/cockroachdb/errors"
	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	zoektutil "github.com/sourcegraph/sourcegraph/internal/search/zoekt"
	"golang.org/x/sync/errgroup"
)

// StructuralSearchFilesInRepos searches a set of repos for a structural pattern.
func StructuralSearchFilesInRepos(ctx context.Context, args *search.TextParameters, stream streaming.Sender) (err error) {
	ctx, stream, cleanup := streaming.WithLimit(ctx, stream, int(args.PatternInfo.FileMatchLimit))
	defer cleanup()

	indexed, err := textSearchRequest(ctx, args, zoektutil.MissingRepoRevStatus(stream))
	if err != nil {
		return err
	}

	g, ctx := errgroup.WithContext(ctx)

	if args.Mode != search.SearcherOnly {
		// Run structural search on indexed repositories (fulfilled via searcher).
		g.Go(func() error {
			repos := make([]*search.RepositoryRevisions, 0, len(indexed.Repos()))
			for _, repo := range indexed.Repos() {
				repos = append(repos, repo)
			}
			return callSearcherOverRepos(ctx, args, stream, repos, true)
		})
	}

	// Concurrently run over all unindexed repos for structural search.
	g.Go(func() error {
		return callSearcherOverRepos(ctx, args, stream, indexed.Unindexed, false)
	})

	return g.Wait()
}

// StructuralSearchFilesInRepoBatch is a convenience function around
// StructuralSearchFilesInRepos which collects the results from the stream.
func StructuralSearchFilesInReposBatch(ctx context.Context, args *search.TextParameters) ([]*result.FileMatch, streaming.Stats, error) {
	matches, stats, err := streaming.CollectStream(func(stream streaming.Sender) error {
		return StructuralSearchFilesInRepos(ctx, args, stream)
	})

	fms, fmErr := matchesToFileMatches(matches)
	if fmErr != nil && err == nil {
		err = errors.Wrap(fmErr, "StructuralSearchFilesInReposBatch failed to convert results")
	}
	return fms, stats, err
}

func StructuralSearch(ctx context.Context, args *search.TextParameters, stream streaming.Sender) error {
	if args.PatternInfo.FileMatchLimit != search.DefaultMaxSearchResults {
		// Service structural search via SearchFilesInRepos when we have
		// an explicit `count` value that differs from the default value
		// (e.g., user sets higher counts).
		return StructuralSearchFilesInRepos(ctx, args, stream)
	}

	// For structural search with default limits we retry if we get no results.
	fileMatches, stats, err := StructuralSearchFilesInReposBatch(ctx, args)

	if len(fileMatches) == 0 && err == nil {
		// No results for structural search? Automatically search again and force Zoekt
		// to resolve more potential file matches by setting a higher FileMatchLimit.
		patternCopy := *(args.PatternInfo)
		patternCopy.FileMatchLimit = 1000
		argsCopy := *args
		argsCopy.PatternInfo = &patternCopy
		args = &argsCopy

		fileMatches, stats, err = StructuralSearchFilesInReposBatch(ctx, args)

		if len(fileMatches) == 0 {
			// Still no results? Give up.
			log15.Warn("Structural search gives up after more exhaustive attempt. Results may have been missed.")
			stats.IsLimitHit = false // Ensure we don't display "Show more".
		}
	}

	matches := make([]result.Match, 0, len(fileMatches))
	for _, fm := range fileMatches {
		matches = append(matches, fm)
	}

	stream.Send(streaming.SearchEvent{
		Results: matches,
		Stats:   stats,
	})
	return err
}
