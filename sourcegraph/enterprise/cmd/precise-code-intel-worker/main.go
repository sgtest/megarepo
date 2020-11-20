package main

import (
	"context"
	"database/sql"
	"errors"
	"log"
	"net/http"
	"time"

	"github.com/aws/aws-sdk-go/aws/awserr"
	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/worker"
	eiauthz "github.com/sourcegraph/sourcegraph/enterprise/internal/authz"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver"
	store "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/uploadstore"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/db"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"github.com/sourcegraph/sourcegraph/internal/logging"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

const addr = ":3188"

func main() {
	config := &Config{}
	config.Load()

	env.Lock()
	env.HandleHelpFlag()
	logging.Init()
	tracer.Init()
	trace.Init(true)

	if err := config.Validate(); err != nil {
		log.Fatalf("Failed to load config: %s", err)
	}

	// Initialize tracing/metrics
	observationContext := &observation.Context{
		Logger:     log15.Root(),
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}

	// Start debug server
	go debugserver.NewServerRoutine().Start()

	// Connect to databases
	db := mustInitializeDB()
	codeIntelDB := mustInitializeCodeIntelDB()

	// Initialize stores
	dbStore := store.NewWithDB(db, observationContext)
	lsifStore := lsifstore.NewStore(codeIntelDB, observationContext)
	gitserverClient := gitserver.New(dbStore, observationContext)

	uploadStore, err := uploadstore.CreateLazy(context.Background(), config.UploadStoreConfig, observationContext)
	if err != nil {
		log.Fatalf("Failed to create upload store: %s", err)
	}
	if err := initializeUploadStore(context.Background(), uploadStore); err != nil {
		log.Fatalf("Failed to initialize upload store: %s", err)
	}

	// Initialize metrics
	mustRegisterQueueMetric(observationContext, dbStore)

	// Initialize worker
	worker := worker.NewWorker(
		&worker.DBStoreShim{dbStore},
		&worker.LSIFStoreShim{lsifStore},
		uploadStore,
		gitserverClient,
		config.WorkerPollInterval,
		config.WorkerConcurrency,
		config.WorkerBudget,
		makeWorkerMetrics(observationContext),
	)

	// Initialize health server
	server := httpserver.NewFromAddr(addr, &http.Server{Handler: httpserver.NewHandler(nil)})

	// Go!
	goroutine.MonitorBackgroundRoutines(context.Background(), worker, server)
}

func mustInitializeDB() *sql.DB {
	postgresDSN := conf.Get().ServiceConnections.PostgresDSN
	conf.Watch(func() {
		if newDSN := conf.Get().ServiceConnections.PostgresDSN; postgresDSN != newDSN {
			log.Fatalf("Detected database DSN change, restarting to take effect: %s", newDSN)
		}
	})

	if err := dbconn.SetupGlobalConnection(postgresDSN); err != nil {
		log.Fatalf("Failed to connect to frontend database: %s", err)
	}

	//
	// START FLAILING

	ctx := context.Background()
	go func() {
		for range time.NewTicker(5 * time.Second).C {
			allowAccessByDefault, authzProviders, _, _ := eiauthz.ProvidersFromConfig(ctx, conf.Get(), db.ExternalServices)
			authz.SetProviders(allowAccessByDefault, authzProviders)
		}
	}()

	// END FLAILING
	//

	return dbconn.Global
}

func mustInitializeCodeIntelDB() *sql.DB {
	postgresDSN := conf.Get().ServiceConnections.CodeIntelPostgresDSN
	conf.Watch(func() {
		if newDSN := conf.Get().ServiceConnections.CodeIntelPostgresDSN; postgresDSN != newDSN {
			log.Fatalf("Detected codeintel database DSN change, restarting to take effect: %s", newDSN)
		}
	})

	db, err := dbconn.New(postgresDSN, "_codeintel")
	if err != nil {
		log.Fatalf("Failed to connect to codeintel database: %s", err)
	}

	if err := dbconn.MigrateDB(db, "codeintel"); err != nil {
		log.Fatalf("Failed to perform codeintel database migration: %s", err)
	}

	return db
}

func mustRegisterQueueMetric(observationContext *observation.Context, dbStore *store.Store) {
	observationContext.Registerer.MustRegister(prometheus.NewGaugeFunc(prometheus.GaugeOpts{
		Name: "src_upload_queue_uploads_total",
		Help: "Total number of queued in the queued state.",
	}, func() float64 {
		count, err := dbStore.QueueSize(context.Background())
		if err != nil {
			log15.Error("Failed to determine queue size", "err", err)
		}

		return float64(count)
	}))
}

func makeWorkerMetrics(observationContext *observation.Context) workerutil.WorkerMetrics {
	metrics := metrics.NewOperationMetrics(
		observationContext.Registerer,
		"upload_queue_processor",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of records processed"),
	)

	return workerutil.WorkerMetrics{
		HandleOperation: observationContext.Operation(observation.Op{
			Name:         "Processor.Process",
			MetricLabels: []string{"process"},
			Metrics:      metrics,
		}),
	}
}

func initializeUploadStore(ctx context.Context, uploadStore uploadstore.Store) error {
	for {
		if err := uploadStore.Init(ctx); err == nil || !isRequestError(err) {
			return err
		}

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(250 * time.Millisecond):
		}
	}
}

func isRequestError(err error) bool {
	for err != nil {
		if e, ok := err.(awserr.Error); ok {
			if e.Code() == "RequestError" {
				return true
			}
		}

		err = errors.Unwrap(err)
	}

	return false
}
