package shared

import (
	"context"
	"time"

	"github.com/sourcegraph/log"

	gcptraceexporter "github.com/GoogleCloudPlatform/opentelemetry-operations-go/exporter/trace"
	"go.opentelemetry.io/contrib/detectors/gcp"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.7.0"

	"github.com/sourcegraph/sourcegraph/internal/trace/policy"
	"github.com/sourcegraph/sourcegraph/internal/tracer/oteldefaults"
	"github.com/sourcegraph/sourcegraph/internal/tracer/oteldefaults/exporters"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// maybeEnableTracing configures OpenTelemetry tracing if the GOOGLE_CLOUD_PROJECT is set.
// It differs from Sourcegraph's default tracing because we need to export directly to GCP,
// and the use case is more niche as a standalone service.
//
// Based on https://cloud.google.com/trace/docs/setup/go-ot
func maybeEnableTracing(ctx context.Context, logger log.Logger, config TraceConfig) (func(), error) {
	// Set globals
	policy.SetTracePolicy(config.Policy)
	otel.SetTextMapPropagator(oteldefaults.Propagator())
	otel.SetErrorHandler(otel.ErrorHandlerFunc(func(err error) {
		logger.Debug("OpenTelemetry error", log.Error(err))
	}))

	// Initialize exporter
	var exporter sdktrace.SpanExporter
	if config.GCPProjectID != "" {
		logger.Info("initializing GCP trace exporter", log.String("projectID", config.GCPProjectID))
		var err error
		exporter, err = gcptraceexporter.New(
			gcptraceexporter.WithProjectID(config.GCPProjectID),
			gcptraceexporter.WithErrorHandler(otel.ErrorHandlerFunc(func(err error) {
				logger.Warn("gcptraceexporter error", log.Error(err))
			})),
		)
		if err != nil {
			return nil, errors.Wrap(err, "gcptraceexporter.New")
		}
	} else {
		logger.Info("initializing OTLP exporter")
		var err error
		exporter, err = exporters.NewOTLPExporter(ctx, logger)
		if err != nil {
			return nil, errors.Wrap(err, "exporters.NewOTLPExporter")
		}
	}

	// Identify your application using resource detection
	res, err := resource.New(ctx,
		// Use the GCP resource detector to detect information about the GCP platform
		resource.WithDetectors(gcp.NewDetector()),
		// Keep the default detectors
		resource.WithTelemetrySDK(),
		// Add your own custom attributes to identify your application
		resource.WithAttributes(
			semconv.ServiceNameKey.String("cody-gateway"),
			semconv.ServiceVersionKey.String(version.Version()),
		),
	)
	if err != nil {
		return nil, errors.Wrap(err, "resource.New")
	}

	// Create and set global tracer
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithBatcher(exporter),
		sdktrace.WithResource(res),
	)
	otel.SetTracerProvider(tp)

	logger.Info("tracing configured")
	return func() {
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()

		start := time.Now()
		logger.Info("Shutting down tracing")
		if err := tp.ForceFlush(shutdownCtx); err != nil {
			logger.Warn("error occurred force-flushing traces", log.Error(err))
		}
		if err := tp.Shutdown(shutdownCtx); err != nil {
			logger.Warn("error occured shutting down tracing", log.Error(err))
		}
		logger.Info("Tracing shut down", log.Duration("elapsed", time.Since(start)))
	}, nil
}
