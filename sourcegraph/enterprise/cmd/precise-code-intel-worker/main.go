package main

import (
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/resetter"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/server"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/worker"
	bundles "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/client"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/sqliteutil"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
)

func main() {
	env.Lock()
	env.HandleHelpFlag()
	tracer.Init()

	sqliteutil.MustRegisterSqlite3WithPcre()

	var (
		bundleManagerURL   = mustGet(rawBundleManagerURL, "PRECISE_CODE_INTEL_BUNDLE_MANAGER_URL")
		workerPollInterval = mustParseInterval(rawWorkerPollInterval, "PRECISE_CODE_INTEL_WORKER_POLL_INTERVAL")
		resetInterval      = mustParseInterval(rawResetInterval, "PRECISE_CODE_INTEL_RESET_INTERVAL")
	)

	observationContext := &observation.Context{
		Logger:     log15.Root(),
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}

	store := store.NewObserved(mustInitializeStore(), observationContext)
	MustRegisterQueueMonitor(observationContext.Registerer, store)
	workerMetrics := worker.NewWorkerMetrics(prometheus.DefaultRegisterer)
	resetterMetrics := resetter.NewResetterMetrics(prometheus.DefaultRegisterer)
	server := server.New()

	uploadResetter := resetter.UploadResetter{
		Store:         store,
		ResetInterval: resetInterval,
		Metrics:       resetterMetrics,
	}

	worker := worker.NewWorker(
		store,
		bundles.New(bundleManagerURL),
		gitserver.DefaultClient,
		workerPollInterval,
		workerMetrics,
	)

	go server.Start()
	go uploadResetter.Run()
	go worker.Start()
	go debugserver.Start()

	// Attempt to clean up after first shutdown signal
	signals := make(chan os.Signal, 2)
	signal.Notify(signals, syscall.SIGINT, syscall.SIGHUP)
	<-signals

	go func() {
		// Insta-shutdown on a second signal
		<-signals
		os.Exit(0)
	}()

	server.Stop()
	worker.Stop()
}

func mustInitializeStore() store.Store {
	postgresDSN := conf.Get().ServiceConnections.PostgresDSN
	conf.Watch(func() {
		if newDSN := conf.Get().ServiceConnections.PostgresDSN; postgresDSN != newDSN {
			log.Fatalf("detected repository DSN change, restarting to take effect: %s", newDSN)
		}
	})

	store, err := store.New(postgresDSN)
	if err != nil {
		log.Fatalf("failed to initialize store: %s", err)
	}

	return store
}
