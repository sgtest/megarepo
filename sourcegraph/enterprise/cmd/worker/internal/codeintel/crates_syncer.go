package codeintel

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/shared/init/codeintel"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/autoindexing"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/policies"
	gitserverc "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/gitserver"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type cratesSyncerJob struct{}

func NewCratesSyncerJob() job.Job {
	return &cratesSyncerJob{}
}

func (j *cratesSyncerJob) Description() string {
	return "crates.io syncer"
}

func (j *cratesSyncerJob) Config() []env.Config {
	return nil
}

func (j *cratesSyncerJob) Routines(_ context.Context, observationCtx *observation.Context) ([]goroutine.BackgroundRoutine, error) {
	db, err := workerdb.InitDB(observationCtx)
	if err != nil {
		return nil, err
	}

	codeintelDB, err := codeintel.InitDB(observationCtx)
	if err != nil {
		return nil, err
	}

	gitserverClient := gitserver.NewClient()
	codeintelGitserver := gitserverc.NewWithGitserverClient(observationCtx, db, gitserverClient)
	uploadsSvc := uploads.NewService(observationCtx, db, codeintelDB, codeintelGitserver)
	policiesSvc := policies.NewService(observationCtx, db, uploadsSvc, codeintelGitserver)
	dependenciesService := dependencies.NewService(observationCtx, db)
	autoindexingSvc := autoindexing.NewService(observationCtx, db, dependenciesService, policiesSvc, codeintelGitserver)

	return dependencies.CrateSyncerJob(
		observationCtx,
		autoindexingSvc,
		dependenciesService,
		gitserverClient,
		db.ExternalServices(),
	), nil
}
