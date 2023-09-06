package search

import (
	"context"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/service"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/store"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/types"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

// newExhaustiveSearchRepoWorker creates a background routine that periodically runs the exhaustive search of a repo.
func newExhaustiveSearchRepoWorker(
	ctx context.Context,
	observationCtx *observation.Context,
	workerStore dbworkerstore.Store[*types.ExhaustiveSearchRepoJob],
	exhaustiveSearchStore *store.Store,
	newSearcher service.NewSearcher,
	config config,
) goroutine.BackgroundRoutine {
	handler := &exhaustiveSearchRepoHandler{
		logger:      log.Scoped("exhaustive-search-repo", "The background worker running exhaustive searches on a repository"),
		store:       exhaustiveSearchStore,
		newSearcher: newSearcher,
	}

	opts := workerutil.WorkerOptions{
		Name:              "exhaustive_search_repo_worker",
		Description:       "runs the exhaustive search on a repository",
		NumHandlers:       5,
		Interval:          config.WorkerInterval,
		HeartbeatInterval: 15 * time.Second,
		Metrics:           workerutil.NewMetrics(observationCtx, "exhaustive_search_repo_worker"),
	}

	return dbworker.NewWorker[*types.ExhaustiveSearchRepoJob](ctx, workerStore, handler, opts)
}

type exhaustiveSearchRepoHandler struct {
	logger      log.Logger
	store       *store.Store
	newSearcher service.NewSearcher
}

var _ workerutil.Handler[*types.ExhaustiveSearchRepoJob] = &exhaustiveSearchRepoHandler{}

func (h *exhaustiveSearchRepoHandler) Handle(ctx context.Context, logger log.Logger, record *types.ExhaustiveSearchRepoJob) error {
	repoRevSpec := types.RepositoryRevSpec{
		Repository:        record.RepoID,
		RevisionSpecifier: record.RefSpec,
	}

	parent, err := h.store.GetExhaustiveSearchJob(ctx, record.SearchJobID)
	if err != nil {
		return err
	}

	q, err := h.newSearcher.NewSearch(ctx, parent.Query)
	if err != nil {
		return err
	}

	repoRevisions, err := q.ResolveRepositoryRevSpec(ctx, repoRevSpec)
	if err != nil {
		return err
	}

	tx, err := h.store.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	for _, repoRev := range repoRevisions {
		_, err := tx.CreateExhaustiveSearchRepoRevisionJob(ctx, types.ExhaustiveSearchRepoRevisionJob{
			SearchRepoJobID: record.ID,
			Revision:        repoRev.Revision,
		})
		if err != nil {
			return err
		}
	}

	return nil
}
