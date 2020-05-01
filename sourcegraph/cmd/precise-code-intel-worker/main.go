package main

import (
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-worker/internal/worker"
	bundles "github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/client"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/db"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/sqliteutil"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
)

func main() {
	env.Lock()
	env.HandleHelpFlag()
	tracer.Init()

	sqliteutil.MustRegisterSqlite3WithPcre()

	var (
		pollInterval     = mustParseInterval(rawPollInterval, "PRECISE_CODE_INTEL_POLL_INTERVAL")
		bundleManagerURL = mustGet(rawBundleManagerURL, "PRECISE_CODE_INTEL_BUNDLE_MANAGER_URL")
	)

	db := mustInitializeDatabase()

	workerImpl := worker.New(worker.WorkerOpts{
		DB:                  db,
		BundleManagerClient: bundles.New(bundleManagerURL),
		GitserverClient:     gitserver.DefaultClient,
		PollInterval:        pollInterval,
	})

	go func() { _ = workerImpl.Start() }()
	go debugserver.Start()
	waitForSignal()
}

func mustInitializeDatabase() db.DB {
	postgresDSN := conf.Get().ServiceConnections.PostgresDSN
	conf.Watch(func() {
		if newDSN := conf.Get().ServiceConnections.PostgresDSN; postgresDSN != newDSN {
			log.Fatalf("Detected repository DSN change, restarting to take effect: %s", newDSN)
		}
	})

	db, err := db.New(postgresDSN)
	if err != nil {
		log.Fatalf("failed to initialize db store: %s", err)
	}

	return db
}

func waitForSignal() {
	signals := make(chan os.Signal, 2)
	signal.Notify(signals, syscall.SIGINT, syscall.SIGHUP)

	for i := 0; i < 2; i++ {
		<-signals
	}

	os.Exit(0)
}
