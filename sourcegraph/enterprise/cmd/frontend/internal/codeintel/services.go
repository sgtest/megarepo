package codeintel

import (
	"context"
	"database/sql"
	"fmt"
	"log"
	"sync"

	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/api"
	codeintelapi "github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/api"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver"
	store "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/uploadstore"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

var services struct {
	dbStore         *store.Store
	lsifStore       *lsifstore.Store
	uploadStore     uploadstore.Store
	gitserverClient *gitserver.Client
	api             *codeintelapi.CodeIntelAPI
	err             error
}

var once sync.Once

func initServices(ctx context.Context) error {
	once.Do(func() {
		if err := config.UploadStoreConfig.Validate(); err != nil {
			services.err = fmt.Errorf("Failed to load config: %s", err)
			return
		}

		// Initialize tracing/metrics
		observationContext := &observation.Context{
			Logger:     log15.Root(),
			Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
			Registerer: prometheus.DefaultRegisterer,
		}

		// Connect to database
		codeIntelDB := mustInitializeCodeIntelDB()

		// Initialize stores
		dbStore := store.NewWithDB(dbconn.Global, observationContext)
		lsifStore := lsifstore.NewStore(codeIntelDB, observationContext)

		uploadStore, err := uploadstore.Create(context.Background(), config.UploadStoreConfig, observationContext)
		if err != nil {
			log.Fatalf("Failed to initialize upload store: %s", err)
		}
		gitserverClient := gitserver.New(observationContext)

		// Initialize internal codeintel API
		api := codeintelapi.New(&api.DBStoreShim{dbStore}, lsifStore, &codeintelapi.GitserverClientShim{gitserverClient}, observationContext)

		services.dbStore = dbStore
		services.lsifStore = lsifStore
		services.uploadStore = uploadStore
		services.gitserverClient = gitserverClient
		services.api = api
	})

	return services.err
}

func mustInitializeCodeIntelDB() *sql.DB {
	postgresDSN := conf.Get().ServiceConnections.CodeIntelPostgresDSN
	conf.Watch(func() {
		if newDSN := conf.Get().ServiceConnections.CodeIntelPostgresDSN; postgresDSN != newDSN {
			log.Fatalf("Detected database DSN change, restarting to take effect: %s", newDSN)
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
