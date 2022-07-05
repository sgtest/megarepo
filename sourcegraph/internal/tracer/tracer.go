package tracer

import (
	"io"

	"github.com/opentracing/opentracing-go"
	"github.com/sourcegraph/log"
	"go.uber.org/automaxprocs/maxprocs"

	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/hostname"
	"github.com/sourcegraph/sourcegraph/internal/trace/policy"
	"github.com/sourcegraph/sourcegraph/internal/version"
)

// options control the behavior of a TracerType
type options struct {
	TracerType
	externalURL string
	debug       bool
	// these values are not configurable by site config
	resource log.Resource
}

type TracerType string

const (
	None          TracerType = "none"
	OpenTracing   TracerType = "opentracing"
	OpenTelemetry TracerType = "opentelemetry"
)

// isSetByUser returns true if the TracerType is one supported by the schema
// should be kept in sync with ObservabilityTracing.Type in schema/site.schema.json
func (t TracerType) isSetByUser() bool {
	switch t {
	case OpenTracing, OpenTelemetry:
		return true
	}
	return false
}

// Init should be called from the main function of service
func Init(logger log.Logger, c conftypes.WatchableSiteConfig) {
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

	initTracer(logger, &options{resource: resource}, c)
}

// initTracer is a helper that should be called exactly once (from Init).
func initTracer(logger log.Logger, opts *options, c conftypes.WatchableSiteConfig) {
	globalTracer := newSwitchableTracer(logger.Scoped("global", "the global tracer"))
	opentracing.SetGlobalTracer(globalTracer)

	// Initially everything is disabled since we haven't read conf yet. This variable is
	// also updated to compare against new version of configuration.
	oldOpts := options{
		resource: opts.resource,
		// the values below may change
		TracerType:  None,
		debug:       false,
		externalURL: "",
	}

	// Watch loop
	go c.Watch(func() {
		var (
			siteConfig = c.SiteConfig()
			debug      = false
			setTracer  = None
		)

		if tracingConfig := siteConfig.ObservabilityTracing; tracingConfig != nil {
			debug = tracingConfig.Debug

			// If sampling policy is set, update the strategy and set our tracer to be
			// OpenTracing by default.
			previousPolicy := policy.GetTracePolicy()
			switch p := policy.TracePolicy(tracingConfig.Sampling); p {
			case policy.TraceAll, policy.TraceSelective:
				policy.SetTracePolicy(p)
				setTracer = OpenTracing // enable the defualt tracer type
			default:
				policy.SetTracePolicy(policy.TraceNone)
			}
			if newPolicy := policy.GetTracePolicy(); newPolicy != previousPolicy {
				logger.Info("updating TracePolicy",
					log.String("oldValue", string(previousPolicy)),
					log.String("newValue", string(newPolicy)))
			}

			// If the tracer type is configured, also set the tracer type
			if t := TracerType(tracingConfig.Type); t.isSetByUser() {
				setTracer = t
			}
		}

		opts := options{
			TracerType:  setTracer,
			externalURL: siteConfig.ExternalURL,
			debug:       debug,
			// Stays the same
			resource: oldOpts.resource,
		}
		if opts == oldOpts {
			// Nothing changed
			return
		}

		// update old opts for comparison
		oldOpts = opts

		// create the new tracer and assign it globally
		tracerLogger := logger.With(
			log.String("tracerType", string(opts.TracerType)),
			log.Bool("debug", opts.debug))
		t, closer, err := newTracer(tracerLogger, &opts)
		if err != nil {
			tracerLogger.Warn("failed to initialize tracer", log.Error(err))
			return
		}
		globalTracer.set(tracerLogger, t, closer, opts.debug)
	})
}

// newTracer creates a tracer based on options
func newTracer(logger log.Logger, opts *options) (opentracing.Tracer, io.Closer, error) {
	logger.Debug("configuring tracer")

	switch opts.TracerType {
	case OpenTracing:
		return newJaegerTracer(logger, opts)

	case OpenTelemetry:
		return newOTelBridgeTracer(logger, opts)

	default:
		return opentracing.NoopTracer{}, nil, nil
	}
}
