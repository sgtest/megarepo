package repo

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/background/repo"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type repoEmbeddingSchedulerJob struct{}

func NewRepoEmbeddingSchedulerJob() job.Job {
	return &repoEmbeddingSchedulerJob{}
}

func (r repoEmbeddingSchedulerJob) Description() string {
	return "resolves policies and schedules repos for embedding"
}

func (r repoEmbeddingSchedulerJob) Config() []env.Config {
	return nil
}

func (r repoEmbeddingSchedulerJob) Routines(_ context.Context, observationCtx *observation.Context) ([]goroutine.BackgroundRoutine, error) {
	db, err := workerdb.InitDB(observationCtx)
	if err != nil {
		return nil, err
	}

	ctx := context.Background()

	return []goroutine.BackgroundRoutine{
		newRepoEmbeddingScheduler(ctx, gitserver.NewClient(), db, repo.NewRepoEmbeddingJobsStore(db)),
	}, nil

}

func newRepoEmbeddingScheduler(
	ctx context.Context,
	gitserverClient gitserver.Client,
	db database.DB,
	repoEmbeddingJobsStore repo.RepoEmbeddingJobsStore,
) goroutine.BackgroundRoutine {
	enqueueActive := goroutine.HandlerFunc(
		func(ctx context.Context) error {
			embeddableRepos, err := repoEmbeddingJobsStore.GetEmbeddableRepos(ctx)
			if err != nil {
				return err
			}

			// get repo names from embeddable repos
			var repoIDs []api.RepoID
			for _, embeddable := range embeddableRepos {
				repoIDs = append(repoIDs, embeddable.ID)
			}
			repos, err := db.Repos().GetByIDs(ctx, repoIDs...)
			if err != nil {
				return err
			}
			var repoNames []api.RepoName
			for _, r := range repos {
				repoNames = append(repoNames, r.Name)
			}

			return embeddings.ScheduleRepositoriesForEmbedding(ctx, repoNames, db, repoEmbeddingJobsStore, gitserverClient)
		})
	return goroutine.NewPeriodicGoroutine(ctx,
		"repoEmbeddingSchedulerJob",
		"resolves embedding policies and schedules jobs to embed repos",
		1*time.Minute,
		enqueueActive)
}
