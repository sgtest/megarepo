package codeintel

import (
	"context"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/shared/init/codeintel"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/autoindexing"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater"

	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type autoindexingDependencyScheduler struct{}

func NewAutoindexingDependencySchedulerJob() job.Job {
	return &autoindexingDependencyScheduler{}
}

func (j *autoindexingDependencyScheduler) Description() string {
	return ""
}

func (j *autoindexingDependencyScheduler) Config() []env.Config {
	return []env.Config{
		autoindexing.ConfigDependencyIndexInst,
	}
}

func (j *autoindexingDependencyScheduler) Routines(startupCtx context.Context, logger log.Logger) ([]goroutine.BackgroundRoutine, error) {
	services, err := codeintel.InitServices()
	if err != nil {
		return nil, err
	}

	db, err := workerdb.InitDBWithLogger(logger)
	if err != nil {
		return nil, err
	}

	return autoindexing.NewDependencyIndexSchedulers(
		db,
		services.UploadsService,
		services.DependenciesService,
		services.AutoIndexingService,
		repoupdater.DefaultClient,
		observation.ContextWithLogger(logger),
	), nil
}
