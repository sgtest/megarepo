package dependencies

import (
	"sync"

	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"golang.org/x/sync/semaphore"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/internal/lockfiles"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

var (
	svc     *Service
	svcOnce sync.Once
)

var (
	lockfilesSemaphoreWeight = env.MustGetInt("CODEINTEL_DEPENDENCIES_LOCKFILES_SEMAPHORE_WEIGHT", 64, "The maximum number of concurrent routines parsing lockfile contents.")
	syncerSemaphoreWeight    = env.MustGetInt("CODEINTEL_DEPENDENCIES_LOCKFILES_SYNCER_WEIGHT", 64, "The maximum number of concurrent routines actively syncing repositories.")
	enablePreciseQueries     = env.Get("CODEINTEL_DEPENDENCIES_ENABLE_PRECISE_QUERIES", "", "Enables queries of precise code intelligence results from the database from the dependencies service.") != ""

	// For mocking in tests
	lockfileIndexingEnabled = conf.CodeIntelLockfileIndexingEnabled
)

// GetService creates or returns an already-initialized dependencies service. If the service is
// new, it will use the given database handle and git/syncer instances.
func GetService(db database.DB, gitService GitService, syncer Syncer) *Service {
	logger := log.Scoped("dependencies.service", "codeintel dependencies service")

	svcOnce.Do(func() {
		storeObservationCtx := &observation.Context{
			Logger:     log.Scoped("dependencies.store", "dependencies store"),
			Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
			Registerer: prometheus.DefaultRegisterer,
		}
		var store store.Store = store.New(db, storeObservationCtx)

		observationCtx := &observation.Context{
			Logger:     logger,
			Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
			Registerer: prometheus.DefaultRegisterer,
		}

		lockfilesService := lockfiles.GetService(gitService)
		lockfilesSemaphore := semaphore.NewWeighted(int64(lockfilesSemaphoreWeight))
		syncerSemaphore := semaphore.NewWeighted(int64(syncerSemaphoreWeight))

		svc = newService(
			store,
			gitService,
			lockfilesService,
			lockfilesSemaphore,
			syncer,
			syncerSemaphore,
			observationCtx,
		)
	})

	return svc
}

// TestService creates a fresh dependencies service with the given database handle and git/syncer
// instances.
func TestService(db database.DB, gitService GitService, syncer Syncer) *Service {
	lockfilesService := lockfiles.GetService(gitService)
	lockfilesSemaphore := semaphore.NewWeighted(64)
	syncerSemaphore := semaphore.NewWeighted(64)
	store := store.New(db, &observation.TestContext)

	return newService(
		store,
		gitService,
		lockfilesService,
		lockfilesSemaphore,
		syncer,
		syncerSemaphore,
		&observation.TestContext,
	)
}
