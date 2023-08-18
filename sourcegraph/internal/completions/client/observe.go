package client

import (
	"context"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func newObservedClient(inner types.CompletionsClient) *observedClient {
	observationCtx := observation.NewContext(log.Scoped("completions", "completions client"))
	ops := newOperations(observationCtx)
	return &observedClient{
		inner: inner,
		ops:   ops,
	}
}

type observedClient struct {
	inner types.CompletionsClient
	ops   *operations
}

var _ types.CompletionsClient = (*observedClient)(nil)

func (o *observedClient) Stream(ctx context.Context, feature types.CompletionsFeature, params types.CompletionRequestParameters, send types.SendCompletionEvent) (err error) {
	ctx, tr, endObservation := o.ops.stream.With(ctx, &err, observation.Args{
		Attrs:             append(params.Attrs(feature), attribute.String("feature", string(feature))),
		MetricLabelValues: []string{params.Model},
	})
	defer endObservation(1, observation.Args{})

	tracedSend := func(event types.CompletionResponse) error {
		if event.StopReason != "" {
			tr.AddEvent("stopped", attribute.String("reason", event.StopReason))
		} else {
			tr.AddEvent("completion", attribute.Int("len", len(event.Completion)))
		}
		return send(event)
	}

	return o.inner.Stream(ctx, feature, params, tracedSend)
}

func (o *observedClient) Complete(ctx context.Context, feature types.CompletionsFeature, params types.CompletionRequestParameters) (resp *types.CompletionResponse, err error) {
	ctx, _, endObservation := o.ops.complete.With(ctx, &err, observation.Args{
		Attrs:             append(params.Attrs(feature), attribute.String("feature", string(feature))),
		MetricLabelValues: []string{params.Model},
	})
	defer endObservation(1, observation.Args{})

	return o.inner.Complete(ctx, feature, params)
}

type operations struct {
	stream   *observation.Operation
	complete *observation.Operation
}

var (
	durationBuckets = []float64{0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 25.0, 30.0, 40.0}
	streamMetrics   = metrics.NewREDMetrics(
		prometheus.DefaultRegisterer,
		"completions_stream",
		metrics.WithLabels("model"),
		metrics.WithDurationBuckets(durationBuckets),
	)
	completeMetrics = metrics.NewREDMetrics(
		prometheus.DefaultRegisterer,
		"completions_complete",
		metrics.WithLabels("model"),
		metrics.WithDurationBuckets(durationBuckets),
	)
)

func newOperations(observationCtx *observation.Context) *operations {
	streamOp := observationCtx.Operation(observation.Op{
		Metrics: streamMetrics,
		Name:    "completions.stream",
	})
	completeOp := observationCtx.Operation(observation.Op{
		Metrics: completeMetrics,
		Name:    "completions.complete",
	})
	return &operations{
		stream:   streamOp,
		complete: completeOp,
	}
}
