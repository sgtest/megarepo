package scheduler

import (
	"context"
	"time"

	"github.com/lib/pq"

	"github.com/keegancsmith/sqlf"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/discovery"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/pipeline"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/types"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type BaseJob struct {
	ID              int
	State           string
	FailureMessage  *string
	QueuedAt        time.Time
	StartedAt       *time.Time
	FinishedAt      *time.Time
	ProcessAfter    *time.Time
	NumResets       int
	NumFailures     int
	LastHeartbeatAt time.Time
	ExecutionLogs   []workerutil.ExecutionLogEntry
	WorkerHostname  string
	Cancel          bool
	backfillId      int
}

func (b *BaseJob) RecordID() int {
	return b.ID
}

var baseJobColumns = []*sqlf.Query{
	sqlf.Sprintf("id"),
	sqlf.Sprintf("state"),
	sqlf.Sprintf("failure_message"),
	sqlf.Sprintf("queued_at"),
	sqlf.Sprintf("started_at"),
	sqlf.Sprintf("finished_at"),
	sqlf.Sprintf("process_after"),
	sqlf.Sprintf("num_resets"),
	sqlf.Sprintf("num_failures"),
	sqlf.Sprintf("last_heartbeat_at"),
	sqlf.Sprintf("execution_logs"),
	sqlf.Sprintf("worker_hostname"),
	sqlf.Sprintf("cancel"),
	sqlf.Sprintf("backfill_id"),
}

func scanBaseJob(s dbutil.Scanner) (*BaseJob, error) {
	var job BaseJob
	var executionLogs []dbworkerstore.ExecutionLogEntry

	if err := s.Scan(
		&job.ID,
		&job.State,
		&job.FailureMessage,
		&job.QueuedAt,
		&job.StartedAt,
		&job.FinishedAt,
		&job.ProcessAfter,
		&job.NumResets,
		&job.NumFailures,
		&job.LastHeartbeatAt,
		pq.Array(&executionLogs),
		&job.WorkerHostname,
		&job.Cancel,
		&dbutil.NullInt{N: &job.backfillId},
	); err != nil {
		return nil, err
	}

	for _, entry := range executionLogs {
		job.ExecutionLogs = append(job.ExecutionLogs, workerutil.ExecutionLogEntry(entry))
	}

	return &job, nil
}

type BackgroundJobMonitor struct {
	inProgressWorker   *workerutil.Worker
	inProgressResetter *dbworker.Resetter
	inProgressStore    dbworkerstore.Store

	newBackfillWorker   *workerutil.Worker
	newBackfillResetter *dbworker.Resetter
	newBackfillStore    dbworkerstore.Store
}

type JobMonitorConfig struct {
	InsightsDB      edb.InsightsDB
	InsightStore    store.Interface
	RepoStore       database.RepoStore
	BackfillRunner  pipeline.Backfiller
	ObsContext      *observation.Context
	AllRepoIterator *discovery.AllReposIterator
}

func NewBackgroundJobMonitor(ctx context.Context, config JobMonitorConfig) *BackgroundJobMonitor {
	monitor := &BackgroundJobMonitor{}

	monitor.inProgressWorker, monitor.inProgressResetter, monitor.inProgressStore = makeInProgressWorker(ctx, config)
	monitor.newBackfillWorker, monitor.newBackfillResetter, monitor.newBackfillStore = makeNewBackfillWorker(ctx, config)

	return monitor
}

func (s *BackgroundJobMonitor) Routines() []goroutine.BackgroundRoutine {
	return []goroutine.BackgroundRoutine{
		s.inProgressWorker,
		s.inProgressResetter,
		s.newBackfillWorker,
		s.newBackfillResetter,
	}
}

type SeriesReader interface {
	GetDataSeriesByID(ctx context.Context, id int) (*types.InsightSeries, error)
}

type Scheduler struct {
	backfillStore *BackfillStore
}

func NewScheduler(db edb.InsightsDB) *Scheduler {
	return &Scheduler{backfillStore: NewBackfillStore(db)}
}

func enqueueBackfill(ctx context.Context, handle basestore.TransactableHandle, backfill *SeriesBackfill) error {
	if backfill == nil || backfill.Id == 0 {
		return errors.New("invalid series backfill")
	}
	return basestore.NewWithHandle(handle).Exec(ctx, sqlf.Sprintf("insert into insights_background_jobs (backfill_id) VALUES (%s)", backfill.Id))
}

func (s *Scheduler) With(other basestore.ShareableStore) *Scheduler {
	return &Scheduler{backfillStore: s.backfillStore.With(other)}
}

func (s *Scheduler) InitialBackfill(ctx context.Context, series types.InsightSeries) (_ *SeriesBackfill, err error) {
	tx, err := s.backfillStore.Transact(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { err = tx.Done(err) }()

	bf, err := tx.NewBackfill(ctx, series)
	if err != nil {
		return nil, errors.Wrap(err, "NewBackfill")
	}

	err = enqueueBackfill(ctx, tx.Handle(), bf)
	if err != nil {
		return nil, errors.Wrap(err, "enqueueBackfill")
	}
	return bf, nil
}
