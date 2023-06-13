package executormultiqueue

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	dbstore "github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/internal/batches"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/internal/executorqueue"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/autoindexing"
	executortypes "github.com/sourcegraph/sourcegraph/enterprise/internal/executor/types"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

type multiqueueMetricsReporterJob struct{}

var _ job.Job = &multiqueueMetricsReporterJob{}

func NewMultiqueueMetricsReporterJob() job.Job {
	return &multiqueueMetricsReporterJob{}
}

func (j *multiqueueMetricsReporterJob) Description() string {
	return "executor push-based metrics reporting multiqueue routines"
}

func (j *multiqueueMetricsReporterJob) Config() []env.Config {
	return []env.Config{
		configInst,
	}
}

func (j *multiqueueMetricsReporterJob) Routines(_ context.Context, observationCtx *observation.Context) ([]goroutine.BackgroundRoutine, error) {
	db, err := workerdb.InitDB(observationCtx)
	if err != nil {
		return nil, err
	}
	codeIntelStore := dbworkerstore.New(observationCtx, db.Handle(), autoindexing.IndexWorkerStoreOptions)
	batchesStore, err := dbstore.InitBatchSpecWorkspaceExecutionWorkerStore()
	if err != nil {
		return nil, err
	}

	multiqueueMetricsReporter, err := executorqueue.NewMultiqueueMetricReporter(
		executortypes.ValidQueueNames,
		configInst.MetricsConfig,
		codeIntelStore.QueuedCount,
		batchesStore.QueuedCount,
	)
	if err != nil {
		return nil, err
	}

	return []goroutine.BackgroundRoutine{multiqueueMetricsReporter}, nil
}

type janitorConfig struct {
	MetricsConfig *executorqueue.Config
}

var configInst = &janitorConfig{}

func (c *janitorConfig) Load() {
	metricsConfig := executorqueue.InitMetricsConfig()
	metricsConfig.Load()
	c.MetricsConfig = metricsConfig
}

func (c *janitorConfig) Validate() error {
	return c.MetricsConfig.Validate()
}
