package batches

import (
	"context"

	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/internal/batches/janitor"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/internal/executorqueue"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

type janitorJob struct{}

func NewJanitorJob() job.Job {
	return &janitorJob{}
}

func (j *janitorJob) Description() string {
	return ""
}

func (j *janitorJob) Config() []env.Config {
	return []env.Config{janitorConfigInst}
}

func (j *janitorJob) Routines(_ context.Context, logger log.Logger) ([]goroutine.BackgroundRoutine, error) {
	observationContext := &observation.Context{
		Logger:     logger.Scoped("routines", "janitor job routines"),
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}
	workCtx := actor.WithInternalActor(context.Background())

	bstore, err := InitStore()
	if err != nil {
		return nil, err
	}

	janitorMetrics := janitor.NewMetrics(observationContext)

	reconcilerStore, err := InitReconcilerWorkerStore()
	if err != nil {
		return nil, err
	}
	bulkOperationStore, err := InitBulkOperationWorkerStore()
	if err != nil {
		return nil, err
	}
	workspaceExecutionStore, err := InitBatchSpecWorkspaceExecutionWorkerStore()
	if err != nil {
		return nil, err
	}
	workspaceResolutionStore, err := InitBatchSpecResolutionWorkerStore()
	if err != nil {
		return nil, err
	}

	executorMetricsReporter, err := executorqueue.NewMetricReporter(observationContext, "batches", workspaceExecutionStore, janitorConfigInst.MetricsConfig)
	if err != nil {
		return nil, err
	}

	routines := []goroutine.BackgroundRoutine{
		executorMetricsReporter,

		janitor.NewReconcilerWorkerResetter(
			reconcilerStore,
			janitorMetrics,
		),
		janitor.NewBulkOperationWorkerResetter(
			bulkOperationStore,
			janitorMetrics,
		),
		janitor.NewBatchSpecWorkspaceExecutionWorkerResetter(
			workspaceExecutionStore,
			janitorMetrics,
		),
		janitor.NewBatchSpecWorkspaceResolutionWorkerResetter(
			workspaceResolutionStore,
			janitorMetrics,
		),

		janitor.NewSpecExpirer(workCtx, bstore),
		janitor.NewCacheEntryCleaner(workCtx, bstore),
	}

	return routines, nil
}
