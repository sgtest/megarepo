package scheduler

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/autoindexing"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func NewScheduler(
	autoindexingSvc *autoindexing.Service,
	dbStore DBStore,
	policyMatcher PolicyMatcher,
	observationContext *observation.Context,
) goroutine.BackgroundRoutine {
	m := metrics.NewREDMetrics(
		observationContext.Registerer,
		"codeintel_index_scheduler",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of method invocations."),
	)

	handleIndexScheduler := observationContext.Operation(observation.Op{
		Name:              "codeintel.indexing.HandleIndexSchedule",
		MetricLabelValues: []string{"HandleIndexSchedule"},
		Metrics:           m,
	})

	return goroutine.NewPeriodicGoroutineWithMetrics(context.Background(), ConfigInst.Interval, &scheduler{
		autoindexingSvc: autoindexingSvc,
		dbStore:         dbStore,
		policyMatcher:   policyMatcher,
	}, handleIndexScheduler)
}
