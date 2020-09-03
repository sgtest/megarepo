package main

import (
	"log"

	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-bundle-manager/internal/janitor"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-bundle-manager/internal/paths"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-bundle-manager/internal/readers"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-bundle-manager/internal/server"
	sqlitereader "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/persistence/sqlite"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/logging"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/sqliteutil"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
)

func main() {
	env.Lock()
	env.HandleHelpFlag()
	logging.Init()
	tracer.Init()
	trace.Init(true)

	sqliteutil.MustRegisterSqlite3WithPcre()

	var (
		bundleDir           = mustGet(rawBundleDir, "PRECISE_CODE_INTEL_BUNDLE_DIR")
		readerDataCacheSize = mustParseInt(rawReaderDataCacheSize, "PRECISE_CODE_INTEL_CONNECTION_DATA_CACHE_CAPACITY")
		desiredPercentFree  = mustParsePercent(rawDesiredPercentFree, "PRECISE_CODE_INTEL_DESIRED_PERCENT_FREE")
		janitorInterval     = mustParseInterval(rawJanitorInterval, "PRECISE_CODE_INTEL_JANITOR_INTERVAL")
		maxUploadAge        = mustParseInterval(rawMaxUploadAge, "PRECISE_CODE_INTEL_MAX_UPLOAD_AGE")
		maxUploadPartAge    = mustParseInterval(rawMaxUploadPartAge, "PRECISE_CODE_INTEL_MAX_UPLOAD_PART_AGE")
		maxDatabasePartAge  = mustParseInterval(rawMaxDatabasePartAge, "PRECISE_CODE_INTEL_MAX_DATABASE_PART_AGE")
		disableJanitor      = mustParseBool(rawDisableJanitor, "PRECISE_CODE_INTEL_DISABLE_JANITOR")
	)

	storeCache, err := sqlitereader.NewStoreCache(readerDataCacheSize)
	if err != nil {
		log.Fatalf("failed to initialize reader cache: %s", err)
	}

	if err := paths.PrepDirectories(bundleDir); err != nil {
		log.Fatalf("failed to prepare directories: %s", err)
	}

	if err := paths.Migrate(bundleDir); err != nil {
		log.Fatalf("failed to migrate paths: %s", err)
	}

	if err := readers.Migrate(bundleDir, storeCache); err != nil {
		log.Fatalf("failed to migrate readers: %s", err)
	}

	observationContext := &observation.Context{
		Logger:     log15.Root(),
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}

	store := store.NewObserved(mustInitializeStore(), observationContext)
	metrics.MustRegisterDiskMonitor(bundleDir)

	server := server.New(bundleDir, storeCache, observationContext)
	janitorMetrics := janitor.NewJanitorMetrics(prometheus.DefaultRegisterer)
	janitor := janitor.New(store, bundleDir, desiredPercentFree, janitorInterval, maxUploadAge, maxUploadPartAge, maxDatabasePartAge, janitorMetrics)

	routines := []goroutine.BackgroundRoutine{
		server,
	}

	if !disableJanitor {
		routines = append(routines, janitor)
	} else {
		log15.Warn("Janitor process is disabled.")
	}

	go debugserver.Start()
	goroutine.MonitorBackgroundRoutines(routines...)
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
