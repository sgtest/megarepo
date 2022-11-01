package background

import (
	"context"
	"os"
	"strconv"
	"time"

	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel"

	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background/limiter"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background/pings"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background/queryrunner"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/compression"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/discovery"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/pipeline"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/priority"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/scheduler"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbcache"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
)

// GetBackgroundJobs is the main entrypoint which starts background jobs for code insights. It is
// called from the worker service.
func GetBackgroundJobs(ctx context.Context, logger log.Logger, mainAppDB database.DB, insightsDB edb.InsightsDB) []goroutine.BackgroundRoutine {
	insightPermStore := store.NewInsightPermissionStore(mainAppDB)
	insightsStore := store.New(insightsDB, insightPermStore)

	// Create a base store to be used for storing worker state. We store this in the main app Postgres
	// DB, not the insights DB (which we use only for storing insights data.)
	workerBaseStore := basestore.NewWithHandle(mainAppDB.Handle())

	// Create basic metrics for recording information about background jobs.
	observationContext := &observation.Context{
		Logger:     logger.Scoped("background", "insights background jobs"),
		Tracer:     &trace.Tracer{TracerProvider: otel.GetTracerProvider()},
		Registerer: prometheus.DefaultRegisterer,
	}

	insightsMetadataStore := store.NewInsightStore(insightsDB)
	featureFlagStore := mainAppDB.FeatureFlags()
	backfillerV2Enabled := false
	backfillerV2Flag, err := featureFlagStore.GetFeatureFlag(ctx, "insights-backfiller-v2")
	if err == nil && backfillerV2Flag != nil && backfillerV2Flag.Bool.Value {
		backfillerV2Enabled = true
	}

	// Start background goroutines for all of our workers.
	// The query runner worker is started in a separate routine so it can benefit from horizontal scaling.
	routines := []goroutine.BackgroundRoutine{
		// Register the background goroutine which discovers and enqueues insights work.
		newInsightEnqueuer(ctx, workerBaseStore, insightsMetadataStore, featureFlagStore, observationContext),

		// TODO(slimsag): future: register another worker here for webhook querying.
	}

	// todo(insights) add setting to disable this indexer
	routines = append(routines, compression.NewCommitIndexerWorker(ctx, mainAppDB, insightsDB, time.Now, observationContext))

	// Register the background goroutine which discovers historical gaps in data and enqueues
	// work to fill them - if not disabled.
	disableHistorical, _ := strconv.ParseBool(os.Getenv("DISABLE_CODE_INSIGHTS_HISTORICAL"))
	if !disableHistorical {
		if backfillerV2Enabled {
			searchRateLimiter := limiter.SearchQueryRate()
			historicRateLimiter := limiter.HistoricalWorkRate()
			backfillConfig := pipeline.BackfillerConfig{
				CompressionPlan:         compression.NewHistoricalFilter(true, time.Now().Add(-1*365*24*time.Hour), edb.NewInsightsDBWith(insightsStore)),
				SearchHandlers:          queryrunner.GetSearchHandlers(),
				InsightStore:            insightsStore,
				CommitClient:            discovery.NewGitCommitClient(mainAppDB),
				SearchPlanWorkerLimit:   1,
				SearchRunnerWorkerLimit: 12,
				SearchRateLimiter:       searchRateLimiter,
				HistoricRateLimiter:     historicRateLimiter,
			}
			backfillRunner := pipeline.NewDefaultBackfiller(backfillConfig)
			config := scheduler.JobMonitorConfig{
				InsightsDB:     insightsDB,
				InsightStore:   insightsStore,
				RepoStore:      mainAppDB.Repos(),
				BackfillRunner: backfillRunner,
				ObsContext:     observationContext,
				AllRepoIterator: discovery.NewAllReposIterator(dbcache.NewIndexableReposLister(observationContext.Logger, mainAppDB.Repos()),
					mainAppDB.Repos(),
					time.Now,
					envvar.SourcegraphDotComMode(),
					15*time.Minute,
					&prometheus.CounterOpts{
						Namespace: "src",
						Name:      "insight_backfill_new_index_repositories_analyzed",
						Help:      "Counter of the number of repositories analyzed in the backfiller new state.",
					}),
				CostAnalyzer: priority.DefaultQueryAnalyzer(),
			}
			monitor := scheduler.NewBackgroundJobMonitor(ctx, config)
			routines = append(routines, monitor.Routines()...)
		} else {
			routines = append(routines, newInsightHistoricalEnqueuer(ctx, workerBaseStore, insightsMetadataStore, insightsStore, featureFlagStore, observationContext))
		}

	}

	// this flag will allow users to ENABLE the settings sync job. This is a last resort option if for some reason the new GraphQL API does not work. This
	// should not be published as an option externally, and will be deprecated as soon as possible.
	enableSync, _ := strconv.ParseBool(os.Getenv("ENABLE_CODE_INSIGHTS_SETTINGS_STORAGE"))
	if enableSync {
		observationContext.Logger.Info("Enabling Code Insights Settings Storage - This is a deprecated functionality!")
		routines = append(routines, discovery.NewMigrateSettingInsightsJob(ctx, mainAppDB, insightsDB))
	}
	routines = append(
		routines,
		pings.NewInsightsPingEmitterJob(ctx, mainAppDB, insightsDB),
		NewInsightsDataPrunerJob(ctx, mainAppDB, insightsDB),
		NewLicenseCheckJob(ctx, mainAppDB, insightsDB),
		NewBackfillCompletedCheckJob(ctx, mainAppDB, insightsDB),
	)

	return routines
}

// GetBackgroundQueryRunnerJob is the main entrypoint for starting the background jobs for code
// insights query runner. It is called from the worker service.
func GetBackgroundQueryRunnerJob(ctx context.Context, logger log.Logger, mainAppDB database.DB, insightsDB edb.InsightsDB) []goroutine.BackgroundRoutine {
	insightPermStore := store.NewInsightPermissionStore(mainAppDB)
	insightsStore := store.New(insightsDB, insightPermStore)

	// Create a base store to be used for storing worker state. We store this in the main app Postgres
	// DB, not the insights DB (which we use only for storing insights data.)
	workerBaseStore := basestore.NewWithHandle(mainAppDB.Handle())
	repoStore := mainAppDB.Repos()

	// Create basic metrics for recording information about background jobs.
	observationContext := &observation.Context{
		Logger:     logger.Scoped("background", "background query runner job"),
		Tracer:     &trace.Tracer{TracerProvider: otel.GetTracerProvider()},
		Registerer: prometheus.DefaultRegisterer,
	}
	queryRunnerWorkerMetrics, queryRunnerResetterMetrics := newWorkerMetrics(observationContext, "query_runner_worker")

	workerStore := queryrunner.CreateDBWorkerStore(workerBaseStore, observationContext)
	seachQueryLimiter := limiter.SearchQueryRate()

	return []goroutine.BackgroundRoutine{
		// Register the query-runner worker and resetter, which executes search queries and records
		// results to the insights DB.
		queryrunner.NewWorker(ctx, logger.Scoped("queryrunner.Worker", ""), workerStore, insightsStore, repoStore, queryRunnerWorkerMetrics, seachQueryLimiter),
		queryrunner.NewResetter(ctx, logger.Scoped("queryrunner.Resetter", ""), workerStore, queryRunnerResetterMetrics),
		queryrunner.NewCleaner(ctx, workerBaseStore, observationContext),
	}
}

// newWorkerMetrics returns a basic set of metrics to be used for a worker and its resetter:
//
//   - WorkerMetrics records worker operations & number of jobs.
//   - ResetterMetrics records the number of jobs that got reset because workers timed out / took too
//     long.
//
// Individual insights workers may then _also_ want to register their own metrics, if desired, in
// their NewWorker functions.
func newWorkerMetrics(observationContext *observation.Context, workerName string) (workerutil.WorkerObservability, dbworker.ResetterMetrics) {
	workerMetrics := workerutil.NewMetrics(observationContext, workerName+"_processor")
	resetterMetrics := dbworker.NewMetrics(observationContext, workerName)
	return workerMetrics, *resetterMetrics
}
