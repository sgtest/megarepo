package shared

import (
	"context"
	"database/sql"
	"net/http"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/syntactic_indexing/jobstore"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/live"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func Main(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, config Config) error {
	logger := observationCtx.Logger

	if err := keyring.Init(ctx); err != nil {
		return errors.Wrap(err, "initializing keyring")
	}

	logger.Info("Syntactic code intel worker running",
		log.String("path to scip-syntax CLI", config.IndexingWorkerConfig.CliPath),
		log.String("API address", config.ListenAddress))

	db := initDB(observationCtx, "syntactic-code-intel-indexer")

	jobStore, err := jobstore.NewStoreWithDB(observationCtx, db)
	if err != nil {
		return errors.Wrap(err, "initializing worker store")
	}

	indexingWorker := NewIndexingWorker(ctx, observationCtx, jobStore, *config.IndexingWorkerConfig)

	// Initialize health server
	server := httpserver.NewFromAddr(config.ListenAddress, &http.Server{
		ReadTimeout:  75 * time.Second,
		WriteTimeout: 10 * time.Minute,
		Handler:      httpserver.NewHandler(nil),
	})

	// Go!
	return goroutine.MonitorBackgroundRoutines(ctx, server, indexingWorker)
}

func initDB(observationCtx *observation.Context, name string) *sql.DB {
	// This is an internal service, so we rely on the
	// frontend to do authz checks for user requests.
	// Authz checks are enforced by the DB layer
	//
	// This call to SetProviders is here so that calls to GetProviders don't block.
	// Relevant PR: https://github.com/sourcegraph/sourcegraph/pull/15755
	// Relevant issue: https://github.com/sourcegraph/sourcegraph/issues/15962

	authz.SetProviders(true, []authz.Provider{})

	dsn := conf.GetServiceConnectionValueAndRestartOnChange(func(serviceConnections conftypes.ServiceConnections) string {
		return serviceConnections.PostgresDSN
	})

	sqlDB, err := connections.EnsureNewFrontendDB(observationCtx, dsn, name)

	if err != nil {
		log.Scoped("init db ("+name+")").Fatal("Failed to connect to frontend database", log.Error(err))
	}

	return sqlDB
}
