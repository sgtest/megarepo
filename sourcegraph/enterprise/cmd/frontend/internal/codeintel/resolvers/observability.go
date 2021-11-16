package resolvers

import (
	"context"
	"fmt"
	"time"

	"github.com/honeycombio/libhoney-go"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/honey"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

type operations struct {
	definitions               *observation.Operation
	diagnostics               *observation.Operation
	documentation             *observation.Operation
	documentationIDsToPathIDs *observation.Operation
	documentationPage         *observation.Operation
	documentationPathInfo     *observation.Operation
	documentationReferences   *observation.Operation
	documentationSearch       *observation.Operation
	hover                     *observation.Operation
	queryResolver             *observation.Operation
	ranges                    *observation.Operation
	references                *observation.Operation
	implementations           *observation.Operation
	stencil                   *observation.Operation

	findClosestDumps *observation.Operation
}

func newOperations(observationContext *observation.Context) *operations {
	metrics := metrics.NewREDMetrics(
		observationContext.Registerer,
		"codeintel_resolvers",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of resolver invocations."),
	)

	op := func(name string) *observation.Operation {
		return observationContext.Operation(observation.Op{
			Name:              fmt.Sprintf("codeintel.resolvers.%s", name),
			MetricLabelValues: []string{name},
			Metrics:           metrics,
		})
	}

	// suboperations do not have their own metrics but do have their
	// own opentracing spans. This allows us to more granularly track
	// the latency for parts of a request without noising up Prometheus.
	subOp := func(name string) *observation.Operation {
		return observationContext.Operation(observation.Op{
			Name: fmt.Sprintf("codeintel.resolvers.%s", name),
		})
	}

	return &operations{
		definitions:               op("Definitions"),
		diagnostics:               op("Diagnostics"),
		documentation:             op("Documentation"),
		documentationIDsToPathIDs: op("DocumentationIDsToPathIDs"),
		documentationPage:         op("DocumentationPage"),
		documentationPathInfo:     op("DocumentationPathInfo"),
		documentationReferences:   op("DocumentationReferences"),
		documentationSearch:       op("DocumentationSearch"),
		hover:                     op("Hover"),
		queryResolver:             op("QueryResolver"),
		ranges:                    op("Ranges"),
		references:                op("References"),
		implementations:           op("Implementations"),
		stencil:                   op("Stencil"),

		findClosestDumps: subOp("findClosestDumps"),
	}
}

func observeResolver(
	ctx context.Context,
	err *error,
	name string,
	operation *observation.Operation,
	threshold time.Duration,
	observationArgs observation.Args,
) (context.Context, observation.TraceLogger, func()) {
	start := time.Now()
	ctx, traceLog, endObservation := operation.WithAndLogger(ctx, err, observationArgs)

	return ctx, traceLog, func() {
		duration := time.Since(start)
		endObservation(1, observation.Args{})

		if duration >= threshold {
			lowSlowRequest(name, duration, err, observationArgs)
		}
		if honey.Enabled() {
			_ = createHoneyEvent(ctx, name, observationArgs, err, duration).Send()
		}
	}
}

func lowSlowRequest(name string, duration time.Duration, err *error, observationArgs observation.Args) {
	pairs := append(
		observationArgs.LogFieldPairs(),
		"type", name,
		"duration_ms", duration.Milliseconds(),
	)
	if err != nil && *err != nil {
		pairs = append(pairs, "error", (*err).Error())
	}

	log15.Warn("Slow codeintel request", pairs...)
}

func createHoneyEvent(
	ctx context.Context,
	name string,
	observationArgs observation.Args,
	err *error,
	duration time.Duration,
) *libhoney.Event {
	fields := map[string]interface{}{
		"type":        name,
		"duration_ms": duration.Milliseconds(),
	}

	if err != nil && *err != nil {
		fields["error"] = (*err).Error()
	}
	for key, value := range observationArgs.LogFieldMap() {
		fields[key] = value
	}
	if traceID := trace.ID(ctx); traceID != "" {
		fields["trace"] = trace.URL(traceID)
		fields["traceID"] = traceID
	}

	return honey.EventWithFields("codeintel", fields)
}
