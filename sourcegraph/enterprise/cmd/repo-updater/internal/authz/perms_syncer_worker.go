package authz

import (
	"context"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/log"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/group"
)

func MakePermsSyncerWorker(ctx context.Context, observationCtx *observation.Context, syncer permsSyncer) *permsSyncerWorker {
	syncGroups := map[requestType]group.ContextGroup{
		requestTypeUser: group.New().WithContext(ctx).WithMaxConcurrency(syncUsersMaxConcurrency()),
		requestTypeRepo: group.New().WithContext(ctx).WithMaxConcurrency(1),
	}

	return &permsSyncerWorker{
		logger:     observationCtx.Logger.Scoped("PermsSyncerWorker", "Permission sync worker"),
		syncer:     syncer,
		syncGroups: syncGroups,
	}
}

type permsSyncer interface {
	syncPerms(ctx context.Context, syncGroups map[requestType]group.ContextGroup, request *syncRequest)
}

type permsSyncerWorker struct {
	logger     log.Logger
	syncer     permsSyncer
	syncGroups map[requestType]group.ContextGroup
}

func (h *permsSyncerWorker) Handle(_ context.Context, _ log.Logger, record *database.PermissionSyncJob) error {
	reqType := requestTypeUser
	reqID := int32(record.UserID)
	if record.RepositoryID != 0 {
		reqType = requestTypeRepo
		reqID = int32(record.RepositoryID)
	}

	h.logger.Info(
		"Handling permission sync job",
		log.String("type", reqType.String()),
		log.Int32("id", reqID),
		log.Int("priority", int(record.Priority)),
	)

	// TODO(naman): when removing old perms syncer, `requestMeta` must be replaced
	// by a new type to include new priority enum. `requestMeta.Priority` itself
	// is not used anywhere in `syncer.syncPerms()`, therefore it is okay for now
	// to pass old priority enum values.
	// `requestQueue` can also be removed as it is only used by the old perms syncer.
	prio := priorityLow
	if record.Priority == database.HighPriorityPermissionSync {
		prio = priorityHigh
	}

	// We use a background context here because right now syncPerms is an async operation.
	//
	// Later we can change the max concurrency on the worker though instead of using
	// the concurrency groups
	syncCtx := actor.WithInternalActor(context.Background())
	h.syncer.syncPerms(syncCtx, h.syncGroups, &syncRequest{requestMeta: &requestMeta{
		Priority: prio,
		Type:     reqType,
		ID:       reqID,
		Options: authz.FetchPermsOptions{
			InvalidateCaches: record.InvalidateCaches,
		},
		// TODO(sashaostrikov): Fill this out
		NoPerms: false,
	}})

	return nil
}

func MakeStore(observationCtx *observation.Context, dbHandle basestore.TransactableHandle) dbworkerstore.Store[*database.PermissionSyncJob] {
	return dbworkerstore.New(observationCtx, dbHandle, dbworkerstore.Options[*database.PermissionSyncJob]{
		Name:              "permission_sync_job_worker_store",
		TableName:         "permission_sync_jobs",
		ColumnExpressions: database.PermissionSyncJobColumns,
		Scan:              dbworkerstore.BuildWorkerScan(database.ScanPermissionSyncJob),
		// NOTE(naman): the priority order to process the queue is as follows:
		// 1. priority: 10(high) > 5(medium) > 0(low)
		// 2. process_after: null(scheduled for immediate processing) > 1 > 2(scheudled for processing at a later time than 1)
		// 3. job_id: 1(old) > 2(enqueued after 1)
		OrderByExpression: sqlf.Sprintf("permission_sync_jobs.priority DESC, permission_sync_jobs.process_after ASC NULLS FIRST, permission_sync_jobs.id ASC"),
		MaxNumResets:      5,
		StalledMaxAge:     time.Second * 30,
	})
}

func MakeWorker(ctx context.Context, observationCtx *observation.Context, workerStore dbworkerstore.Store[*database.PermissionSyncJob], permsSyncer *PermsSyncer) *workerutil.Worker[*database.PermissionSyncJob] {
	handler := MakePermsSyncerWorker(ctx, observationCtx, permsSyncer)

	return dbworker.NewWorker[*database.PermissionSyncJob](ctx, workerStore, handler, workerutil.WorkerOptions{
		Name:              "permission_sync_job_worker",
		Interval:          time.Second, // Poll for a job once per second
		HeartbeatInterval: 10 * time.Second,
		Metrics:           workerutil.NewMetrics(observationCtx, "permission_sync_job_worker"),

		// Process only one job at a time (per instance).
		// TODO(sashaostrikov): This should be changed once the handler above is not async anymore.
		NumHandlers: 1,
	})
}

func MakeResetter(observationCtx *observation.Context, workerStore dbworkerstore.Store[*database.PermissionSyncJob]) *dbworker.Resetter[*database.PermissionSyncJob] {
	return dbworker.NewResetter(observationCtx.Logger, workerStore, dbworker.ResetterOptions{
		Name:     "permission_sync_job_worker_resetter",
		Interval: time.Second * 30, // Check for orphaned jobs every 30 seconds
		Metrics:  dbworker.NewResetterMetrics(observationCtx, "permission_sync_job_worker"),
	})
}
