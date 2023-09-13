package search

import (
	"context"
	"fmt"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/service"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/store"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/types"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// newExhaustiveSearchRepoRevisionWorker creates a background routine that periodically runs the exhaustive search of a revision on a repo.
func newExhaustiveSearchRepoRevisionWorker(
	ctx context.Context,
	observationCtx *observation.Context,
	workerStore dbworkerstore.Store[*types.ExhaustiveSearchRepoRevisionJob],
	exhaustiveSearchStore *store.Store,
	newSearcher service.NewSearcher,
	uploadStore uploadstore.Store,
	config config,
) goroutine.BackgroundRoutine {
	handler := &exhaustiveSearchRepoRevHandler{
		logger:      log.Scoped("exhaustive-search-repo-revision", "The background worker running exhaustive searches on a revision of a repository"),
		store:       exhaustiveSearchStore,
		newSearcher: newSearcher,
		uploadStore: uploadStore,
	}

	opts := workerutil.WorkerOptions{
		Name:              "exhaustive_search_repo_revision_worker",
		Description:       "runs the exhaustive search on a revision of a repository",
		NumHandlers:       5,
		Interval:          config.WorkerInterval,
		HeartbeatInterval: 15 * time.Second,
		Metrics:           workerutil.NewMetrics(observationCtx, "exhaustive_search_repo_revision_worker"),
	}

	return dbworker.NewWorker[*types.ExhaustiveSearchRepoRevisionJob](ctx, workerStore, handler, opts)
}

type exhaustiveSearchRepoRevHandler struct {
	logger      log.Logger
	store       *store.Store
	newSearcher service.NewSearcher
	uploadStore uploadstore.Store
}

var _ workerutil.Handler[*types.ExhaustiveSearchRepoRevisionJob] = &exhaustiveSearchRepoRevHandler{}

func (h *exhaustiveSearchRepoRevHandler) Handle(ctx context.Context, logger log.Logger, record *types.ExhaustiveSearchRepoRevisionJob) error {
	jobID, query, repoRev, err := h.store.GetQueryRepoRev(ctx, record)
	if err != nil {
		return err
	}

	q, err := h.newSearcher.NewSearch(ctx, query)
	if err != nil {
		return err
	}

	csvWriter := service.NewBlobstoreCSVWriter(ctx, h.uploadStore, fmt.Sprintf("%d-%d", jobID, record.ID))

	err = q.Search(ctx, repoRev, csvWriter)
	if closeErr := csvWriter.Close(); closeErr != nil {
		err = errors.Append(err, closeErr)
	}

	return err
}
