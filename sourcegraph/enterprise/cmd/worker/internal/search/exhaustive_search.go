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

// newExhaustiveSearchWorker creates a background routine that periodically runs the exhaustive search.
func newExhaustiveSearchWorker(
	ctx context.Context,
	observationCtx *observation.Context,
	workerStore dbworkerstore.Store[*types.ExhaustiveSearchJob],
	exhaustiveSearchStore *store.Store,
	newSearcher service.NewSearcher,
	config config,
) goroutine.BackgroundRoutine {
	handler := &exhaustiveSearchHandler{
		logger:      log.Scoped("exhaustive-search", "The background worker running exhaustive searches"),
		store:       exhaustiveSearchStore,
		newSearcher: newSearcher,
	}

	opts := workerutil.WorkerOptions{
		Name:              "exhaustive_search_worker",
		Description:       "runs the exhaustive search",
		NumHandlers:       5,
		Interval:          config.WorkerInterval,
		HeartbeatInterval: 15 * time.Second,
		Metrics:           workerutil.NewMetrics(observationCtx, "exhaustive_search_worker"),
	}

	return dbworker.NewWorker[*types.ExhaustiveSearchJob](ctx, workerStore, handler, opts)
}

type exhaustiveSearchHandler struct {
	logger      log.Logger
	store       *store.Store
	newSearcher service.NewSearcher
}

var _ workerutil.Handler[*types.ExhaustiveSearchJob] = &exhaustiveSearchHandler{}

func (h *exhaustiveSearchHandler) Handle(ctx context.Context, logger log.Logger, record *types.ExhaustiveSearchJob) (err error) {
	// TODO observability? read other handlers to see if we are missing stuff

	q, err := h.newSearcher.NewSearch(ctx, record.Query)
	if err != nil {
		return err
	}

	repoRevSpecs, err := q.RepositoryRevSpecs(ctx)
	if err != nil {
		return err
	}

	tx, err := h.store.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	for _, repoRevSpec := range repoRevSpecs {
		_, err := tx.CreateExhaustiveSearchRepoJob(ctx, types.ExhaustiveSearchRepoJob{
			RepoID:      repoRevSpec.Repository,
			RefSpec:     repoRevSpec.RevisionSpecifier,
			SearchJobID: record.ID,
		})
		if err != nil {
			return err
		}
	}

	return nil
}
