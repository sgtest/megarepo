package repo

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/shared/init/codeintel"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	repoembeddingsbg "github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/background/repo"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/embed"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

type repoEmbeddingJob struct{}

func NewRepoEmbeddingJob() job.Job {
	return &repoEmbeddingJob{}
}

func (s *repoEmbeddingJob) Description() string {
	return ""
}

func (s *repoEmbeddingJob) Config() []env.Config {
	return []env.Config{embeddings.EmbeddingsUploadStoreConfigInst}
}

func (s *repoEmbeddingJob) Routines(_ context.Context, observationCtx *observation.Context) ([]goroutine.BackgroundRoutine, error) {
	db, err := workerdb.InitDB(observationCtx)
	if err != nil {
		return nil, err
	}

	uploadStore, err := embeddings.NewEmbeddingsUploadStore(context.Background(), observationCtx, embeddings.EmbeddingsUploadStoreConfigInst)
	if err != nil {
		return nil, err
	}

	services, err := codeintel.InitServices(observationCtx)
	if err != nil {
		return nil, err
	}

	workCtx := actor.WithInternalActor(context.Background())
	return []goroutine.BackgroundRoutine{
		newRepoEmbeddingJobWorker(
			workCtx,
			observationCtx,
			repoembeddingsbg.NewRepoEmbeddingJobWorkerStore(observationCtx, db.Handle()),
			edb.NewEnterpriseDB(db),
			uploadStore,
			gitserver.NewClient(),
			services.ContextService,
			repoembeddingsbg.NewRepoEmbeddingJobsStore(db),
		),
	}, nil
}

func newRepoEmbeddingJobWorker(
	ctx context.Context,
	observationCtx *observation.Context,
	workerStore dbworkerstore.Store[*repoembeddingsbg.RepoEmbeddingJob],
	db edb.EnterpriseDB,
	uploadStore uploadstore.Store,
	gitserverClient gitserver.Client,
	contextService embed.ContextService,
	repoEmbeddingJobsStore repoembeddingsbg.RepoEmbeddingJobsStore,
) *workerutil.Worker[*repoembeddingsbg.RepoEmbeddingJob] {
	handler := &handler{
		db:                     db,
		uploadStore:            uploadStore,
		gitserverClient:        gitserverClient,
		contextService:         contextService,
		repoEmbeddingJobsStore: repoEmbeddingJobsStore,
	}
	return dbworker.NewWorker[*repoembeddingsbg.RepoEmbeddingJob](ctx, workerStore, handler, workerutil.WorkerOptions{
		Name:              "repo_embedding_job_worker",
		Interval:          10 * time.Second, // Poll for a job once every 10 seconds
		NumHandlers:       1,                // Process only one job at a time (per instance)
		HeartbeatInterval: 10 * time.Second,
		Metrics:           workerutil.NewMetrics(observationCtx, "repo_embedding_job_worker"),
	})
}
