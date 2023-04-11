package shared

import (
	"context"
	"database/sql"
	"net/http"
	"time"

	smithyhttp "github.com/aws/smithy-go/transport/http"
	"github.com/sourcegraph/log"

	eiauthz "github.com/sourcegraph/sourcegraph/enterprise/internal/authz"
	srp "github.com/sourcegraph/sourcegraph/enterprise/internal/authz/subrepoperms"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel"
	codeintelshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/lsifuploadstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/live"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/honey"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const addr = ":3188"

func Main(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, config Config) error {
	logger := observationCtx.Logger

	// Initialize tracing/metrics
	observationCtx = observation.NewContext(logger, observation.Honeycomb(&honey.Dataset{
		Name: "codeintel-worker",
	}))

	if err := keyring.Init(ctx); err != nil {
		return errors.Wrap(err, "initializing keyring")
	}

	// Connect to databases
	db := database.NewDB(logger, mustInitializeDB(observationCtx))
	codeIntelDB := mustInitializeCodeIntelDB(observationCtx)

	// Migrations may take a while, but after they're done we'll immediately
	// spin up a server and can accept traffic. Inform external clients we'll
	// be ready for traffic.
	ready()

	// Initialize sub-repo permissions client
	var err error
	authz.DefaultSubRepoPermsChecker, err = srp.NewSubRepoPermsClient(edb.NewEnterpriseDB(db).SubRepoPerms())
	if err != nil {
		return errors.Wrap(err, "creating sub-repo client")
	}

	services, err := codeintel.NewServices(codeintel.ServiceDependencies{
		DB:             db,
		CodeIntelDB:    codeIntelDB,
		ObservationCtx: observationCtx,
	})
	if err != nil {
		return errors.Wrap(err, "creating codeintel services")
	}

	// Initialize stores
	uploadStore, err := lsifuploadstore.New(ctx, observationCtx, config.LSIFUploadStoreConfig)
	if err != nil {
		return errors.Wrap(err, "creating upload store")
	}
	if err := initializeUploadStore(ctx, uploadStore); err != nil {
		return errors.Wrap(err, "initializing upload store")
	}

	// Initialize worker
	worker := uploads.NewUploadProcessorJob(
		observationCtx,
		services.UploadsService,
		db,
		uploadStore,
		config.WorkerConcurrency,
		config.WorkerBudget,
		config.WorkerPollInterval,
		config.MaximumRuntimePerJob,
	)

	// Initialize health server
	server := httpserver.NewFromAddr(addr, &http.Server{
		ReadTimeout:  75 * time.Second,
		WriteTimeout: 10 * time.Minute,
		Handler:      httpserver.NewHandler(nil),
	})

	// Go!
	goroutine.MonitorBackgroundRoutines(ctx, append(worker, server)...)

	return nil
}

func mustInitializeDB(observationCtx *observation.Context) *sql.DB {
	dsn := conf.GetServiceConnectionValueAndRestartOnChange(func(serviceConnections conftypes.ServiceConnections) string {
		return serviceConnections.PostgresDSN
	})
	sqlDB, err := connections.EnsureNewFrontendDB(observationCtx, dsn, "precise-code-intel-worker")
	if err != nil {
		log.Scoped("init db", "Initialize fontend database").Fatal("Failed to connect to frontend database", log.Error(err))
	}

	//
	// START FLAILING

	ctx := context.Background()
	db := database.NewDB(observationCtx.Logger, sqlDB)
	go func() {
		for range time.NewTicker(eiauthz.RefreshInterval()).C {
			allowAccessByDefault, authzProviders, _, _, _ := eiauthz.ProvidersFromConfig(ctx, conf.Get(), db.ExternalServices(), db)
			authz.SetProviders(allowAccessByDefault, authzProviders)
		}
	}()

	// END FLAILING
	//

	return sqlDB
}

func mustInitializeCodeIntelDB(observationCtx *observation.Context) codeintelshared.CodeIntelDB {
	dsn := conf.GetServiceConnectionValueAndRestartOnChange(func(serviceConnections conftypes.ServiceConnections) string {
		return serviceConnections.CodeIntelPostgresDSN
	})
	db, err := connections.EnsureNewCodeIntelDB(observationCtx, dsn, "precise-code-intel-worker")
	if err != nil {
		log.Scoped("init db", "Initialize codeintel database.").Fatal("Failed to connect to codeintel database", log.Error(err))
	}

	return codeintelshared.NewCodeIntelDB(observationCtx.Logger, db)
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
	return errors.HasType(err, &smithyhttp.RequestSendError{})
}
