package lsifstore

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type operations struct {
	getReferences          *observation.Operation
	getImplementations     *observation.Operation
	getHover               *observation.Operation
	getDefinitions         *observation.Operation
	getDiagnostics         *observation.Operation
	getRanges              *observation.Operation
	getStencil             *observation.Operation
	getMonikersByPosition  *observation.Operation
	getPackageInformation  *observation.Operation
	getBulkMonikerResults  *observation.Operation
	getLocationsWithinFile *observation.Operation

	locations *observation.Operation
}

func newOperations(observationContext *observation.Context) *operations {
	metrics := metrics.NewREDMetrics(
		observationContext.Registerer,
		"codeintel_symbols_lsifstore",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of method invocations."),
	)

	op := func(name string) *observation.Operation {
		return observationContext.Operation(observation.Op{
			Name:              fmt.Sprintf("codeintel.symbols.lsifstore.%s", name),
			MetricLabelValues: []string{name},
			Metrics:           metrics,
		})
	}

	// suboperations do not have their own metrics but do have their
	// own opentracing spans. This allows us to more granularly track
	// the latency for parts of a request without noising up Prometheus.
	subOp := func(name string) *observation.Operation {
		return observationContext.Operation(observation.Op{
			Name: fmt.Sprintf("codeintel.lsifstore.%s", name),
		})
	}

	return &operations{
		getReferences:          op("GetReferences"),
		getImplementations:     op("GetImplementations"),
		getHover:               op("GetHover"),
		getDefinitions:         op("GetDefinitions"),
		getDiagnostics:         op("GetDiagnostics"),
		getRanges:              op("GetRanges"),
		getStencil:             op("GetStencil"),
		getMonikersByPosition:  op("GetMonikersByPosition"),
		getPackageInformation:  op("GetPackageInformation"),
		getBulkMonikerResults:  op("GetBulkMonikerResults"),
		getLocationsWithinFile: op("GetLocationsWithinFile"),

		locations: subOp("locations"),
	}
}
