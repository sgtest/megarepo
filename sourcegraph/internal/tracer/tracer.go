package tracer

import (
	"fmt"
	"sync/atomic"
	"text/template"

	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel"
	oteltracesdk "go.opentelemetry.io/otel/sdk/trace"
	oteltrace "go.opentelemetry.io/otel/trace"
	"go.uber.org/automaxprocs/maxprocs"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/hostname"
	"github.com/sourcegraph/sourcegraph/internal/tracer/oteldefaults"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/schema"
)

// options control the behavior of a TracerType
type options struct {
	TracerType
	externalURL string
	// these values are not configurable by site config
	resource log.Resource
}

type TracerType string

const (
	None TracerType = "none"

	// Jaeger exports traces over the Jaeger thrift protocol.
	Jaeger TracerType = "jaeger"

	// OpenTelemetry exports traces over OTLP.
	OpenTelemetry TracerType = "opentelemetry"
)

// DefaultTracerType is the default tracer type if not explicitly set by the user and
// some trace policy is enabled.
const DefaultTracerType = OpenTelemetry

// isSetByUser returns true if the TracerType is one supported by the schema
// should be kept in sync with ObservabilityTracing.Type in schema/site.schema.json
func (t TracerType) isSetByUser() bool {
	switch t {
	case Jaeger, OpenTelemetry:
		return true
	}
	return false
}

type Configuration struct {
	ExternalURL string
	*schema.ObservabilityTracing
}

type ConfigurationSource interface {
	Config() Configuration
}

type WatchableConfigurationSource interface {
	ConfigurationSource

	// Watchable allows the caller to be notified when the configuration changes.
	conftypes.Watchable
}

// Init should be called from the main function of service
func Init(logger log.Logger, c WatchableConfigurationSource) {
	// Tune GOMAXPROCS for kubernetes. All our binaries import this package,
	// so we tune for all of them.
	//
	// TODO it is surprising that we do this here. We should create a standard
	// import for sourcegraph binaries which would have less surprising
	// behaviour.
	if _, err := maxprocs.Set(); err != nil {
		logger.Error("automaxprocs failed", log.Error(err))
	}

	// Resource mirrors the initialization used by our OpenTelemetry logger.
	resource := log.Resource{
		Name:       env.MyName,
		Version:    version.Version(),
		InstanceID: hostname.Get(),
	}

	// Additionally set a dev namespace
	if version.IsDev(version.Version()) {
		resource.Namespace = "dev"
	}

	// Set up initial configurations
	debugMode := &atomic.Bool{}
	provider := newOtelTracerProvider(resource)

	// Create and set up global tracers from provider. We will be making updates to these
	// tracers through the debugMode ref and underlying provider.
	otelTracerProvider := newTracer(logger, provider, debugMode)
	otel.SetTracerProvider(otelTracerProvider)

	// Initially everything is disabled since we haven't read conf yet - start a goroutine
	// that watches for updates to configure the undelrying provider and debugMode.
	go c.Watch(newConfWatcher(logger, c, provider, newOtelSpanProcessor, debugMode))

	// Contribute validation for tracing package
	conf.ContributeWarning(func(c conftypes.SiteConfigQuerier) conf.Problems {
		tracing := c.SiteConfig().ObservabilityTracing
		if tracing == nil || tracing.UrlTemplate == "" {
			return nil
		}
		if _, err := template.New("").Parse(tracing.UrlTemplate); err != nil {
			return conf.NewSiteProblems(fmt.Sprintf("observability.tracing.traceURL is not a valid template: %s", err.Error()))
		}
		return nil
	})
}

func newTracer(logger log.Logger, provider *oteltracesdk.TracerProvider, debugMode *atomic.Bool) oteltrace.TracerProvider {
	propagator := oteldefaults.Propagator()
	otel.SetTextMapPropagator(propagator)

	// Set up logging
	otelLogger := logger.AddCallerSkip(2).Scoped("otel", "OpenTelemetry library")
	otel.SetErrorHandler(otel.ErrorHandlerFunc(func(err error) {
		if debugMode.Load() {
			otelLogger.Warn("error encountered", log.Error(err))
		} else {
			otelLogger.Debug("error encountered", log.Error(err))
		}
	}))
	// Wrap each tracer in additional logging
	return newLoggedOtelTracerProvider(logger, provider, debugMode)
}
