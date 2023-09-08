package shared

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"time"

	"github.com/go-redsync/redsync/v4"
	"github.com/go-redsync/redsync/v4/redis/redigo"
	"github.com/gomodule/redigo/redis"
	"github.com/slack-go/slack"
	"github.com/sourcegraph/conc"
	"github.com/sourcegraph/log"
	"go.opentelemetry.io/contrib/detectors/gcp"
	"go.opentelemetry.io/contrib/instrumentation/net/http/otelhttp"
	"go.opentelemetry.io/otel/sdk/resource"
	semconv "go.opentelemetry.io/otel/semconv/v1.7.0"

	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/auth"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/events"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/limiter"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/notify"
	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/redispool"
	"github.com/sourcegraph/sourcegraph/internal/requestclient"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/actor/anonymous"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/actor/dotcomuser"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/actor/productsubscription"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/dotcom"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/httpapi"
)

func Main(ctx context.Context, obctx *observation.Context, ready service.ReadyFunc, config *Config) error {
	shutdownOtel, err := initOpenTelemetry(ctx, obctx.Logger, config.OpenTelemetry)
	if err != nil {
		return errors.Wrap(err, "initOpenTelemetry")
	}
	defer shutdownOtel()

	var eventLogger events.Logger
	if config.BigQuery.ProjectID != "" {
		eventLogger, err = events.NewBigQueryLogger(config.BigQuery.ProjectID, config.BigQuery.Dataset, config.BigQuery.Table)
		if err != nil {
			return errors.Wrap(err, "create BigQuery event logger")
		}

		// If a buffer is configured, wrap in events.BufferedLogger
		if config.BigQuery.EventBufferSize > 0 {
			eventLogger = events.NewBufferedLogger(obctx.Logger, eventLogger,
				config.BigQuery.EventBufferSize, config.BigQuery.EventBufferWorkers)
		}
	} else {
		eventLogger = events.NewStdoutLogger(obctx.Logger)

		// Useful for testing event logging in a way that has latency that is
		// somewhat similar to BigQuery.
		if os.Getenv("CODY_GATEWAY_BUFFERED_LAGGY_EVENT_LOGGING_FUN_TIMES_MODE") == "true" {
			eventLogger = events.NewBufferedLogger(
				obctx.Logger,
				events.NewDelayedLogger(eventLogger),
				config.BigQuery.EventBufferSize,
				config.BigQuery.EventBufferWorkers)
		}
	}

	// Create an uncached external doer, we never want to cache any responses.
	// Not only is the cache hit rate going to be really low and requests large-ish,
	// but also do we not want to retain any data.
	httpClient, err := httpcli.UncachedExternalClientFactory.Doer()
	if err != nil {
		return errors.Wrap(err, "failed to initialize external http client")
	}

	// Supported actor/auth sources
	sources := actor.NewSources(anonymous.NewSource(config.AllowAnonymous, config.ActorConcurrencyLimit))
	if config.Dotcom.AccessToken != "" {
		// dotcom-based actor sources only if an access token is provided for
		// us to talk with the client
		obctx.Logger.Info("dotcom-based actor sources are enabled")
		dotcomClient := dotcom.NewClient(config.Dotcom.URL, config.Dotcom.AccessToken)
		sources.Add(
			productsubscription.NewSource(
				obctx.Logger,
				rcache.NewWithTTL(fmt.Sprintf("product-subscriptions:%s", productsubscription.SourceVersion), int(config.SourcesCacheTTL.Seconds())),
				dotcomClient,
				config.Dotcom.InternalMode,
				config.ActorConcurrencyLimit,
			),
			dotcomuser.NewSource(obctx.Logger,
				rcache.NewWithTTL(fmt.Sprintf("dotcom-users:%s", dotcomuser.SourceVersion), int(config.SourcesCacheTTL.Seconds())),
				dotcomClient,
				config.ActorConcurrencyLimit,
			),
		)
	} else {
		obctx.Logger.Warn("CODY_GATEWAY_DOTCOM_ACCESS_TOKEN is not set, dotcom-based actor sources are disabled")
	}

	authr := &auth.Authenticator{
		Logger:      obctx.Logger.Scoped("auth", "authentication middleware"),
		EventLogger: eventLogger,
		Sources:     sources,
	}

	rs := newRedisStore(redispool.Cache)

	// Ignore the error because it's already validated in the config.
	dotcomURL, _ := url.Parse(config.Dotcom.URL)
	dotcomURL.Path = ""
	rateLimitNotifier := notify.NewSlackRateLimitNotifier(
		obctx.Logger,
		redispool.Cache,
		dotcomURL.String(),
		notify.Thresholds{
			// Detailed notifications for product subscriptions.
			codygateway.ActorSourceProductSubscription: []int{90, 95, 100},
			// No notifications for individual dotcom users - this can get quite
			// spammy.
			codygateway.ActorSourceDotcomUser: []int{},
		},
		config.ActorRateLimitNotify.SlackWebhookURL,
		func(ctx context.Context, url string, msg *slack.WebhookMessage) error {
			return slack.PostWebhookCustomHTTPContext(ctx, url, otelhttp.DefaultClient, msg)
		},
	)

	// Set up our handler chain, which is run from the bottom up. Application handlers
	// come last.
	handler, err := httpapi.NewHandler(obctx.Logger, eventLogger, rs, httpClient, authr, &httpapi.Config{
		RateLimitNotifier:          rateLimitNotifier,
		AnthropicAccessToken:       config.Anthropic.AccessToken,
		AnthropicAllowedModels:     config.Anthropic.AllowedModels,
		AnthropicMaxTokensToSample: config.Anthropic.MaxTokensToSample,
		OpenAIAccessToken:          config.OpenAI.AccessToken,
		OpenAIOrgID:                config.OpenAI.OrgID,
		OpenAIAllowedModels:        config.OpenAI.AllowedModels,
		FireworksAccessToken:       config.Fireworks.AccessToken,
		FireworksAllowedModels:     config.Fireworks.AllowedModels,
		EmbeddingsAllowedModels:    config.AllowedEmbeddingsModels,
	})
	if err != nil {
		return errors.Wrap(err, "httpapi.NewHandler")
	}

	// Diagnostic layers
	handler = httpapi.NewDiagnosticsHandler(obctx.Logger, handler, config.DiagnosticsSecret, sources)

	// Collect request client for downstream handlers. Outside of dev, we always set up
	// Cloudflare in from of Cody Gateway. This comes first.
	hasCloudflare := !config.InsecureDev
	handler = requestclient.ExternalHTTPMiddleware(handler, hasCloudflare)

	// Initialize our server
	address := fmt.Sprintf(":%d", config.Port)
	server := httpserver.NewFromAddr(address, &http.Server{
		ReadTimeout:  75 * time.Second,
		WriteTimeout: 10 * time.Minute,
		Handler:      handler,
	})

	// Set up redis-based distributed mutex for the source syncer worker
	p, ok := redispool.Store.Pool()
	if !ok {
		return errors.New("real redis is required")
	}
	sourceWorkerMutex := redsync.New(redigo.NewPool(p)).NewMutex("source-syncer-worker",
		// Do not retry endlessly becuase it's very likely that someone else has
		// a long-standing hold on the mutex. We will try again on the next periodic
		// goroutine run.
		redsync.WithTries(1),
		// Expire locks at Nx sync interval to avoid contention while avoiding
		// the lock getting stuck for too long if something happens and to make
		// sure we can extend the lock after a sync. Instances spinning will
		// explicitly release the lock so this is a fallback measure.
		// Note that syncs can take several minutes.
		redsync.WithExpiry(4*config.SourcesSyncInterval))

	// Mark health server as ready and go!
	ready()
	obctx.Logger.Info("service ready", log.String("address", address))

	// Collect background routines
	backgroundRoutines := []goroutine.BackgroundRoutine{
		server,
		sources.Worker(obctx, sourceWorkerMutex, config.SourcesSyncInterval),
	}
	if w, ok := eventLogger.(goroutine.BackgroundRoutine); ok {
		// eventLogger is events.BufferedLogger
		backgroundRoutines = append(backgroundRoutines, w)
	}
	// Block until done
	goroutine.MonitorBackgroundRoutines(ctx, backgroundRoutines...)

	return nil
}

func newRedisStore(store redispool.KeyValue) limiter.RedisStore {
	return &redisStore{
		store: store,
	}
}

type redisStore struct {
	store redispool.KeyValue
}

func (s *redisStore) Incrby(key string, val int) (int, error) {
	return s.store.Incrby(key, val)
}

func (s *redisStore) GetInt(key string) (int, error) {
	i, err := s.store.Get(key).Int()
	if err != nil && err != redis.ErrNil {
		return 0, err
	}
	return i, nil
}

func (s *redisStore) TTL(key string) (int, error) {
	return s.store.TTL(key)
}

func (s *redisStore) Expire(key string, ttlSeconds int) error {
	return s.store.Expire(key, ttlSeconds)
}

func initOpenTelemetry(ctx context.Context, logger log.Logger, config OpenTelemetryConfig) (func(), error) {
	res, err := getOpenTelemetryResource(ctx)
	if err != nil {
		return nil, err
	}

	// Enable tracing, at this point tracing wouldn't have been enabled yet because
	// we run Cody Gateway without conf which means Sourcegraph tracing is not enabled.
	shutdownTracing, err := maybeEnableTracing(ctx,
		logger.Scoped("tracing", "OpenTelemetry tracing"),
		config, res)
	if err != nil {
		return nil, errors.Wrap(err, "maybeEnableTracing")
	}

	shutdownMetrics, err := maybeEnableMetrics(ctx,
		logger.Scoped("metrics", "OpenTelemetry metrics"),
		config, res)
	if err != nil {
		return nil, errors.Wrap(err, "maybeEnableMetrics")
	}

	return func() {
		var wg conc.WaitGroup
		wg.Go(shutdownTracing)
		wg.Go(shutdownMetrics)
		wg.Wait()
	}, nil
}

func getOpenTelemetryResource(ctx context.Context) (*resource.Resource, error) {
	// Identify your application using resource detection
	return resource.New(ctx,
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
}
