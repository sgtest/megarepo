package graphqlbackend

import (
	"context"
	"io/fs"
	"sync"

	"github.com/neelance/parallel"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/inventory"
	"github.com/sourcegraph/sourcegraph/internal/search/job/jobutil"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (srs *searchResultsStats) Languages(ctx context.Context) ([]*languageStatisticsResolver, error) {
	matches, err := srs.getResults(ctx)
	if err != nil {
		return nil, err
	}

	langs, err := searchResultsStatsLanguages(ctx, srs.logger, srs.sr.db, matches)
	if err != nil {
		return nil, err
	}

	wrapped := make([]*languageStatisticsResolver, len(langs))
	for i, lang := range langs {
		wrapped[i] = &languageStatisticsResolver{lang}
	}
	return wrapped, nil
}

func (srs *searchResultsStats) getResults(ctx context.Context) (result.Matches, error) {
	srs.once.Do(func() {
		b, err := query.ToBasicQuery(srs.sr.SearchInputs.Query)
		if err != nil {
			srs.err = err
			return
		}
		j, err := jobutil.NewBasicJob(srs.log, srs.sr.SearchInputs, b)
		if err != nil {
			srs.err = err
			return
		}
		agg := streaming.NewAggregatingStream()
		_, err = j.Run(ctx, srs.sr.JobClients(), agg)
		if err != nil {
			srs.err = err
			return
		}
		srs.results = agg.Results
	})
	return srs.results, srs.err
}

func searchResultsStatsLanguages(ctx context.Context, logger log.Logger, db database.DB, matches []result.Match) ([]inventory.Lang, error) {
	// Batch our operations by repo-commit.
	type repoCommit struct {
		repo     api.RepoID
		commitID api.CommitID
	}

	// Records the work necessary for a batch (repoCommit).
	type fileStatsWork struct {
		fullEntries  []fs.FileInfo     // matched these full files
		partialFiles map[string]uint64 // file with line matches (path) -> count of lines matching
	}

	var (
		repos    = map[api.RepoID]types.MinimalRepo{}
		filesMap = map[repoCommit]*fileStatsWork{}

		run = parallel.NewRun(16)

		allInventories   []inventory.Inventory
		allInventoriesMu sync.Mutex
	)

	// Track the mapping of repo ID -> repo object as we iterate.
	sawRepo := func(repo types.MinimalRepo) {
		if _, ok := repos[repo.ID]; !ok {
			repos[repo.ID] = repo
		}
	}

	// Only count repo matches if all matches are repo matches. Otherwise, it would get confusing
	// because we might have a match of a repo *and* a file in the repo. We would need to avoid
	// double-counting. In this case, we will just count the matching files.
	hasNonRepoMatches := false
	for _, match := range matches {
		if _, ok := match.(*result.RepoMatch); !ok {
			hasNonRepoMatches = true
		}
	}

	for _, res := range matches {
		if fileMatch, ok := res.(*result.FileMatch); ok {
			sawRepo(fileMatch.Repo)
			key := repoCommit{repo: fileMatch.Repo.ID, commitID: fileMatch.CommitID}

			if _, ok := filesMap[key]; !ok {
				filesMap[key] = &fileStatsWork{}
			}

			if len(fileMatch.ChunkMatches) > 0 {
				// Only count matching lines. TODO(sqs): bytes are not counted for these files
				if filesMap[key].partialFiles == nil {
					filesMap[key].partialFiles = map[string]uint64{}
				}
				filesMap[key].partialFiles[fileMatch.Path] += uint64(fileMatch.ChunkMatches.MatchCount())
			} else {
				// Count entire file.
				filesMap[key].fullEntries = append(filesMap[key].fullEntries, &fileInfo{
					path:  fileMatch.Path,
					isDir: false,
				})
			}
		} else if repoMatch, ok := res.(*result.RepoMatch); ok && !hasNonRepoMatches {
			sawRepo(repoMatch.RepoName())
			run.Acquire()
			goroutine.Go(func() {
				defer run.Release()

				repoName := repoMatch.RepoName()
				_, oid, err := gitserver.NewClient(db).GetDefaultBranch(ctx, repoName.Name)
				if err != nil {
					run.Error(err)
					return
				}
				inv, err := backend.NewRepos(logger, db).GetInventory(ctx, repoName.ToRepo(), oid, true)
				if err != nil {
					run.Error(err)
					return
				}
				allInventoriesMu.Lock()
				allInventories = append(allInventories, *inv)
				allInventoriesMu.Unlock()
			})
		} else if _, ok := res.(*result.CommitMatch); ok {
			return nil, errors.New("language statistics do not support diff searches")
		}
	}

	for key_, work_ := range filesMap {
		key := key_
		work := work_
		run.Acquire()
		goroutine.Go(func() {
			defer run.Release()

			invCtx, err := backend.InventoryContext(repos[key.repo].Name, db, key.commitID, true)
			if err != nil {
				run.Error(err)
				return
			}

			// Inventory all full-entry (files and trees) matches together.
			inv, err := invCtx.Entries(ctx, work.fullEntries...)
			if err != nil {
				run.Error(err)
				return
			}
			allInventoriesMu.Lock()
			allInventories = append(allInventories, inv)
			allInventoriesMu.Unlock()

			// Separately inventory each partial-file match because we only increment the language lines
			// by the number of matched lines in the file.
			for partialFile, lines := range work.partialFiles {
				inv, err := invCtx.Entries(ctx,
					fileInfo{path: partialFile, isDir: false},
				)
				if err != nil {
					run.Error(err)
					return
				}
				for i := range inv.Languages {
					inv.Languages[i].TotalLines = lines
				}
				allInventoriesMu.Lock()
				allInventories = append(allInventories, inv)
				allInventoriesMu.Unlock()
			}
		})
	}

	if err := run.Wait(); err != nil {
		return nil, err
	}
	return inventory.Sum(allInventories).Languages, nil
}
