package codeintel

import (
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel"

	"github.com/sourcegraph/sourcegraph/cmd/worker/memo"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/repoupdater"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

// InitGitserverClient initializes and returns a gitserver client.
func InitGitserverClient() (*gitserver.Client, error) {
	return initGitserverClient.Init()
}

var initGitserverClient = memo.NewMemoizedConstructor(func() (*gitserver.Client, error) {
	logger := log.Scoped("client.gitserver", "gitserver client")
	observationContext := &observation.Context{
		Logger:     logger,
		Tracer:     &trace.Tracer{TracerProvider: otel.GetTracerProvider()},
		Registerer: prometheus.DefaultRegisterer,
	}

	db, err := workerdb.Init()
	if err != nil {
		return nil, err
	}

	return gitserver.New(database.NewDB(logger, db), observationContext), nil
})

func InitRepoUpdaterClient() *repoupdater.Client {
	client, _ := initRepoUpdaterClient.Init()
	return client
}

var initRepoUpdaterClient = memo.NewMemoizedConstructor(func() (*repoupdater.Client, error) {
	observationContext := &observation.Context{
		Logger:     log.Scoped("client.repo-updater", "repo-updater client"),
		Tracer:     &trace.Tracer{TracerProvider: otel.GetTracerProvider()},
		Registerer: prometheus.DefaultRegisterer,
	}

	return repoupdater.New(observationContext), nil
})
