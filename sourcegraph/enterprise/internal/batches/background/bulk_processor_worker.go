package background

import (
	"context"
	"database/sql"
	"time"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/sources"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

// bulkProcessorMaxNumRetries is the maximum number of attempts the bulkProcessor
// makes to process a changeset job when it fails.
const bulkProcessorMaxNumRetries = 10

// bulkProcessorMaxNumResets is the maximum number of attempts the bulkProcessor
// makes to process a changeset job when it stalls (process crashes, etc.).
const bulkProcessorMaxNumResets = 60

// newBulkJobWorker creates a dbworker.Worker that fetches enqueued changeset_jobs
// from the database and passes them to the bulk executor for processing.
func newBulkJobWorker(
	ctx context.Context,
	s *store.Store,
	sourcer sources.Sourcer,
	metrics batchChangesMetrics,
) *workerutil.Worker {
	r := &bulkProcessorWorker{sourcer: sourcer, store: s}

	options := workerutil.WorkerOptions{
		Name:        "batches_bulk_processor",
		NumHandlers: 5,
		Interval:    5 * time.Second,
		Metrics:     metrics.bulkProcessorWorkerMetrics,
	}

	workerStore := createBulkJobDBWorkerStore(s)

	worker := dbworker.NewWorker(ctx, workerStore, r.HandlerFunc(), options)
	return worker
}

// newBulkJobWorkerResetter creates a dbworker.Resetter that reenqueues lost jobs
// for processing.
func newBulkJobWorkerResetter(s *store.Store, metrics batchChangesMetrics) *dbworker.Resetter {
	workerStore := createBulkJobDBWorkerStore(s)

	options := dbworker.ResetterOptions{
		Name:     "batches_bulk_worker_resetter",
		Interval: 1 * time.Minute,
		Metrics:  metrics.bulkProcessorWorkerResetterMetrics,
	}

	resetter := dbworker.NewResetter(workerStore, options)
	return resetter
}

func createBulkJobDBWorkerStore(s *store.Store) dbworkerstore.Store {
	return dbworkerstore.New(s.Handle(), dbworkerstore.Options{
		Name:              "batches_bulk_worker_store",
		TableName:         "changeset_jobs",
		ColumnExpressions: store.ChangesetJobColumns,
		Scan:              scanFirstChangesetJobRecord,

		OrderByExpression: sqlf.Sprintf("changeset_jobs.state = 'errored', changeset_jobs.updated_at DESC"),

		StalledMaxAge: 60 * time.Second,
		MaxNumResets:  bulkProcessorMaxNumResets,

		RetryAfter:    5 * time.Second,
		MaxNumRetries: bulkProcessorMaxNumRetries,
	})
}

// scanFirstChangesetJobRecord wraps store.ScanFirstChangesetJob to return a
// generic workerutil.Record.
func scanFirstChangesetJobRecord(rows *sql.Rows, err error) (workerutil.Record, bool, error) {
	return store.ScanFirstChangesetJob(rows, err)
}

// bulkProcessorWorker is a wrapper for the workerutil handlerfunc to create a
// bulkProcessor with a source and store.
type bulkProcessorWorker struct {
	store   *store.Store
	sourcer sources.Sourcer
}

func (b *bulkProcessorWorker) HandlerFunc() dbworker.HandlerFunc {
	return func(ctx context.Context, tx dbworkerstore.Store, record workerutil.Record) error {
		processor := &bulkProcessor{
			sourcer: b.sourcer,
			store:   b.store.With(tx),
		}
		return processor.process(ctx, record.(*btypes.ChangesetJob))
	}
}
