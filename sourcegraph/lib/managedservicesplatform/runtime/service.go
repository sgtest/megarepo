package runtime

import (
	"context"
	"flag"
	"os"

	"cloud.google.com/go/profiler"
	"github.com/getsentry/sentry-go"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/lib/background"
	"github.com/sourcegraph/sourcegraph/lib/managedservicesplatform/runtime/contract"
	"github.com/sourcegraph/sourcegraph/lib/managedservicesplatform/runtime/internal/opentelemetry"
)

type Service[ConfigT any] interface {
	contract.ServiceMetadataProvider
	// Initialize should use given configuration to build a combined background
	// routine (such as background.CombinedRoutine or background.LIFOStopRoutine)
	// that implements starting and stopping the service.
	Initialize(
		ctx context.Context,
		logger log.Logger,
		contract Contract,
		config ConfigT,
	) (background.Routine, error)
}

var showHelp = flag.Bool("help", false, "Show service help text")

// Start handles the entire lifecycle of the program running Service, and should
// be the only thing called in a MSP program's main package, for example:
//
//	runtime.Start[example.Config](example.Service{})
//
// Where example.Config is your runtime.ConfigLoader implementation, and
// example.Service is your runtime.Service implementation.
func Start[
	ConfigT any,
	LoaderT ConfigLoader[ConfigT],
](service Service[ConfigT]) {
	flag.Parse()
	passSanityCheck(service)

	// Resource representing the service
	res := log.Resource{
		Name:       service.Name(),
		Version:    service.Version(),
		Namespace:  "",
		InstanceID: "",
	}

	liblog := log.Init(res, log.NewSentrySink())
	defer liblog.Sync()

	ctx := context.Background()

	// logger should only be used within Start
	logger := log.Scoped("msp.start")

	env, err := contract.ParseEnv(os.Environ())
	if err != nil {
		logger.Fatal("failed to load environment", log.Error(err))
	}

	// Initialize LoaderT implementation as non-zero *ConfigT
	var config LoaderT = new(ConfigT)

	// Load configuration variables from environment
	config.Load(env)
	ctr := contract.New(log.Scoped("msp.contract"), service, env)

	// Fast-exit with configuration facts if requested
	if *showHelp {
		renderHelp(service, env)
		os.Exit(0)
	}

	// Enable Sentry error log reporting
	var sentryEnabled bool
	if ctr.Diagnostics.SentryDSN != nil {
		liblog.Update(func() log.SinksConfig {
			sentryEnabled = true
			return log.SinksConfig{
				Sentry: &log.SentrySink{
					ClientOptions: sentry.ClientOptions{
						Dsn:         *ctr.Diagnostics.SentryDSN,
						Environment: ctr.EnvironmentID,
					},
				},
			}
		})()
	}

	// Check for environment errors
	if err := env.Validate(); err != nil {
		logger.Fatal("environment configuration error encountered", log.Error(err))
	}

	// Initialize things dependent on configuration being loaded
	otelCleanup, err := opentelemetry.Init(ctx, logger.Scoped("otel"), ctr.Diagnostics.OpenTelemetry, res)
	if err != nil {
		logger.Fatal("failed to initialize OpenTelemetry", log.Error(err))
	}
	defer otelCleanup()

	if ctr.MSP {
		if err := profiler.Start(profiler.Config{
			Service:        service.Name(),
			ServiceVersion: service.Version(),
			// Options used in sourcegraph/sourcegraph
			MutexProfiling: true,
			AllocForceGC:   true,
		}); err != nil {
			// For now, keep this optional and don't prevent startup
			logger.Error("failed to initialize profiler", log.Error(err))
		} else {
			logger.Debug("Cloud Profiler enabled")
		}
	}

	// Initialize the service
	routine, err := service.Initialize(
		ctx,
		log.Scoped("service"),
		ctr,
		*config,
	)
	if err != nil {
		logger.Fatal("service startup failed", log.Error(err))
	}

	// Start service routine, and block until it stops.
	logger.Info("starting service",
		log.Int("port", ctr.Port),
		log.Bool("msp", ctr.MSP),
		log.Bool("sentry", sentryEnabled))
	background.Monitor(ctx, routine)
	logger.Info("service stopped")
}
