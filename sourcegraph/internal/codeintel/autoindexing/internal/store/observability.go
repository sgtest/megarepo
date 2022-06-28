package store

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type operations struct {
	list                           *observation.Operation
	deleteIndexesWithoutRepository *observation.Operation

	staleSourcedCommits  *observation.Operation
	deleteSourcedCommits *observation.Operation
	updateSourcedCommits *observation.Operation
}

func newOperations(observationContext *observation.Context) *operations {
	metrics := metrics.NewREDMetrics(
		observationContext.Registerer,
		"codeintel_autoindexing_store",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of method invocations."),
	)

	op := func(name string) *observation.Operation {
		return observationContext.Operation(observation.Op{
			Name:              fmt.Sprintf("codeintel.autoindexing.store.%s", name),
			MetricLabelValues: []string{name},
			Metrics:           metrics,
		})
	}

	return &operations{
		list:                           op("List"),
		deleteIndexesWithoutRepository: op("DeleteIndexesWithoutRepository"),
		staleSourcedCommits:            op("StaleSourcedCommits"),
		deleteSourcedCommits:           op("DeleteSourcedCommits"),
		updateSourcedCommits:           op("UpdateSourcedCommits"),
	}
}
