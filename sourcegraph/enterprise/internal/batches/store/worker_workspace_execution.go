package store

import (
	"context"
	"database/sql"
	"encoding/json"
	"time"

	"github.com/graph-gophers/graphql-go/relay"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/log"

	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	batcheslib "github.com/sourcegraph/sourcegraph/lib/batches"
	"github.com/sourcegraph/sourcegraph/lib/batches/execution/cache"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// batchSpecWorkspaceExecutionJobStalledJobMaximumAge is the maximum allowable
// duration between updating the state of a job as "processing" and locking the
// record during processing. An unlocked row that is marked as processing
// likely indicates that the executor that dequeued the job has died. There
// should be a nearly-zero delay between these states during normal operation.
const batchSpecWorkspaceExecutionJobStalledJobMaximumAge = time.Second * 25

// batchSpecWorkspaceExecutionJobMaximumNumResets is the maximum number of
// times a job can be reset. If a job's failed attempts counter reaches this
// threshold, it will be moved into "failed" rather than "queued" on its next
// reset.
const batchSpecWorkspaceExecutionJobMaximumNumResets = 3

var batchSpecWorkspaceExecutionWorkerStoreOptions = dbworkerstore.Options{
	Name:              "batch_spec_workspace_execution_worker_store",
	TableName:         "batch_spec_workspace_execution_jobs",
	ColumnExpressions: batchSpecWorkspaceExecutionJobColumnsWithNullQueue.ToSqlf(),
	Scan: func(rows *sql.Rows, err error) (workerutil.Record, bool, error) {
		return scanFirstBatchSpecWorkspaceExecutionJob(rows, err)
	},
	OrderByExpression: sqlf.Sprintf("batch_spec_workspace_execution_jobs.place_in_global_queue"),
	StalledMaxAge:     batchSpecWorkspaceExecutionJobStalledJobMaximumAge,
	MaxNumResets:      batchSpecWorkspaceExecutionJobMaximumNumResets,
	// Explicitly disable retries.
	MaxNumRetries: 0,

	// This view ranks jobs from different users in a round-robin fashion
	// so that no single user can clog the queue.
	ViewName: "batch_spec_workspace_execution_jobs_with_rank batch_spec_workspace_execution_jobs",
}

type BatchSpecWorkspaceExecutionWorkerStore interface {
	dbworkerstore.Store

	FetchCanceled(ctx context.Context, executorName string) (canceledIDs []int, err error)
}

// NewBatchSpecWorkspaceExecutionWorkerStore creates a dbworker store that
// wraps the batch_spec_workspace_execution_jobs table.
func NewBatchSpecWorkspaceExecutionWorkerStore(handle basestore.TransactableHandle, observationContext *observation.Context) BatchSpecWorkspaceExecutionWorkerStore {
	return &batchSpecWorkspaceExecutionWorkerStore{
		Store:              dbworkerstore.NewWithMetrics(handle, batchSpecWorkspaceExecutionWorkerStoreOptions, observationContext),
		observationContext: observationContext,
		logger:             log.Scoped("batch-spec-workspace-execution-worker-store", "The worker store backing the executor queue for Batch Changes"),
	}
}

var _ dbworkerstore.Store = &batchSpecWorkspaceExecutionWorkerStore{}

// batchSpecWorkspaceExecutionWorkerStore is a thin wrapper around
// dbworkerstore.Store that allows us to extract information out of the
// ExecutionLogEntry field and persisting it to separate columns when marking a
// job as complete.
type batchSpecWorkspaceExecutionWorkerStore struct {
	dbworkerstore.Store

	logger log.Logger

	observationContext *observation.Context
}

func (s *batchSpecWorkspaceExecutionWorkerStore) FetchCanceled(ctx context.Context, executorName string) (canceledIDs []int, err error) {
	batchesStore := New(database.NewDBWith(s.logger, s.Store), s.observationContext, nil)

	t := true
	cs, err := batchesStore.ListBatchSpecWorkspaceExecutionJobs(ctx, ListBatchSpecWorkspaceExecutionJobsOpts{
		Cancel:         &t,
		State:          btypes.BatchSpecWorkspaceExecutionJobStateProcessing,
		WorkerHostname: executorName,
		ExcludeRank:    true,
	})
	if err != nil {
		return nil, err
	}

	ids := make([]int, 0, len(cs))
	for _, c := range cs {
		ids = append(ids, c.RecordID())
	}
	return ids, nil
}

type markFinal func(ctx context.Context, tx dbworkerstore.Store) (_ bool, err error)

func (s *batchSpecWorkspaceExecutionWorkerStore) markFinal(ctx context.Context, id int, fn markFinal) (ok bool, err error) {
	batchesStore := New(database.NewDBWith(s.logger, s.Store), s.observationContext, nil)
	tx, err := batchesStore.Transact(ctx)
	if err != nil {
		return false, err
	}
	defer func() {
		// If no matching record was found, revert the tx.
		if !ok && err == nil {
			tx.Done(errors.New("record not found"))
			return
		}
		// If we failed to mark the job as final, we fall back to the
		// non-wrapped functions so that the job does get marked as
		// final/errored if, e.g., parsing the logs failed.
		err = tx.Done(err)
		if err != nil {
			s.logger.Error("marking job as final failed, falling back to base method", log.Int("id", id), log.Error(err))
			// Note: we don't use the transaction.
			ok, err = fn(ctx, s.Store)
		}
	}()

	job, err := tx.GetBatchSpecWorkspaceExecutionJob(ctx, GetBatchSpecWorkspaceExecutionJobOpts{ID: int64(id), ExcludeRank: true})
	if err != nil {
		return false, err
	}

	events, err := logEventsFromLogEntries(job.ExecutionLogs)
	if err != nil {
		return false, err
	}

	stepResults, err := extractCacheEntries(events)
	if err != nil {
		return false, err
	}

	workspace, err := tx.GetBatchSpecWorkspace(ctx, GetBatchSpecWorkspaceOpts{ID: job.BatchSpecWorkspaceID})
	if err != nil {
		return false, err
	}

	spec, err := tx.GetBatchSpec(ctx, GetBatchSpecOpts{ID: workspace.BatchSpecID})
	if err != nil {
		return false, err
	}

	if err := storeCacheResults(ctx, tx, stepResults, spec.UserID); err != nil {
		return false, err
	}

	return fn(ctx, s.Store.With(tx))
}

func (s *batchSpecWorkspaceExecutionWorkerStore) MarkErrored(ctx context.Context, id int, failureMessage string, options dbworkerstore.MarkFinalOptions) (_ bool, err error) {
	return s.markFinal(ctx, id, func(ctx context.Context, tx dbworkerstore.Store) (bool, error) {
		return tx.MarkErrored(ctx, id, failureMessage, options)
	})
}

func (s *batchSpecWorkspaceExecutionWorkerStore) MarkFailed(ctx context.Context, id int, failureMessage string, options dbworkerstore.MarkFinalOptions) (_ bool, err error) {
	return s.markFinal(ctx, id, func(ctx context.Context, tx dbworkerstore.Store) (bool, error) {
		return tx.MarkFailed(ctx, id, failureMessage, options)
	})
}

func (s *batchSpecWorkspaceExecutionWorkerStore) MarkComplete(ctx context.Context, id int, options dbworkerstore.MarkFinalOptions) (ok bool, err error) {
	batchesStore := New(database.NewDBWith(s.logger, s.Store), s.observationContext, nil)

	tx, err := batchesStore.Transact(ctx)
	if err != nil {
		return false, err
	}
	defer func() {
		// If no matching record was found, revert the tx.
		// We don't want to persist side-effects.
		if !ok && err == nil {
			tx.Done(errors.New("record not found"))
			return
		}
		// If we failed to mark the job as completed, we fall back to the
		// non-wrapped store method so that the job is marked as
		// failed if, e.g., parsing the logs failed.
		err = tx.Done(err)
		if err != nil {
			s.logger.Error("Marking job complete failed, falling back to failure", log.Int("id", id), log.Error(err))
			// Note: we don't use the transaction.
			ok, err = s.Store.MarkFailed(ctx, id, err.Error(), options)
		}
	}()

	job, err := tx.GetBatchSpecWorkspaceExecutionJob(ctx, GetBatchSpecWorkspaceExecutionJobOpts{ID: int64(id), ExcludeRank: true})
	if err != nil {
		return false, errors.Wrap(err, "loading batch spec workspace execution job")
	}

	workspace, err := tx.GetBatchSpecWorkspace(ctx, GetBatchSpecWorkspaceOpts{ID: job.BatchSpecWorkspaceID})
	if err != nil {
		return false, errors.Wrap(err, "loading batch spec workspace")
	}

	batchSpec, err := tx.GetBatchSpec(ctx, GetBatchSpecOpts{ID: workspace.BatchSpecID})
	if err != nil {
		return false, errors.Wrap(err, "loading batch spec")
	}

	events, err := logEventsFromLogEntries(job.ExecutionLogs)
	if err != nil {
		return false, errors.Wrap(err, "logEventsFromLogEntries")
	}

	// Impersonate as the user to ensure the repo is still accessible by them.
	ctx = actor.WithActor(ctx, actor.FromUser(batchSpec.UserID))
	repo, err := tx.Repos().Get(ctx, workspace.RepoID)
	if err != nil {
		return false, errors.Wrap(err, "failed to validate repo access")
	}

	stepResults, err := extractCacheEntries(events)
	if err != nil {
		return false, errors.Wrap(err, "failed to extract cache entries")
	}

	// This is a hard-error, every execution must emit at least one of them.
	if len(stepResults) == 0 {
		return false, errors.New("found no step results")
	}

	if err := storeCacheResults(ctx, tx, stepResults, batchSpec.UserID); err != nil {
		return false, err
	}

	// Find the result for the last step. This is the one we'll be building the execution
	// result from.
	var latestStepResult *batcheslib.CacheAfterStepResultMetadata = stepResults[0]
	for _, r := range stepResults {
		if r.Value.StepIndex > latestStepResult.Value.StepIndex {
			latestStepResult = r
		}
	}

	rawSpecs, err := cache.ChangesetSpecsFromCache(
		batchSpec.Spec,
		batcheslib.Repository{
			ID:          string(relay.MarshalID("Repository", repo.ID)),
			Name:        string(repo.Name),
			BaseRef:     workspace.Branch,
			BaseRev:     workspace.Commit,
			FileMatches: workspace.FileMatches,
		},
		latestStepResult.Value,
		workspace.Path,
	)
	if err != nil {
		return false, errors.Wrap(err, "failed to build changeset specs from cache")
	}

	var specs []*btypes.ChangesetSpec
	for _, rawSpec := range rawSpecs {
		changesetSpec, err := btypes.NewChangesetSpecFromSpec(rawSpec)
		if err != nil {
			return false, errors.Wrap(err, "failed to build db changeset specs")
		}
		changesetSpec.BatchSpecID = batchSpec.ID
		changesetSpec.RepoID = repo.ID
		changesetSpec.UserID = batchSpec.UserID

		specs = append(specs, changesetSpec)
	}

	changesetSpecIDs := []int64{}
	if len(specs) > 0 {
		if err := tx.CreateChangesetSpec(ctx, specs...); err != nil {
			return false, errors.Wrap(err, "failed to store changeset specs")
		}
		for _, spec := range specs {
			changesetSpecIDs = append(changesetSpecIDs, spec.ID)
		}
	}

	if err = s.setChangesetSpecIDs(ctx, tx, job.BatchSpecWorkspaceID, changesetSpecIDs); err != nil {
		return false, errors.Wrap(err, "setChangesetSpecIDs")
	}

	return s.Store.With(tx).MarkComplete(ctx, id, options)
}

func (s *batchSpecWorkspaceExecutionWorkerStore) setChangesetSpecIDs(ctx context.Context, tx *Store, batchSpecWorkspaceID int64, changesetSpecIDs []int64) error {
	// Marshal changeset spec IDs for database JSON column.
	m := make(map[int64]struct{}, len(changesetSpecIDs))
	for _, id := range changesetSpecIDs {
		m[id] = struct{}{}
	}
	marshaledIDs, err := json.Marshal(m)
	if err != nil {
		return err
	}

	// Set changeset_spec_ids on the batch_spec_workspace.
	res, err := tx.ExecResult(ctx, sqlf.Sprintf(setChangesetSpecIDsOnBatchSpecWorkspaceQueryFmtstr, marshaledIDs, batchSpecWorkspaceID))
	if err != nil {
		return err
	}

	c, err := res.RowsAffected()
	if err != nil {
		return err
	}

	if c != 1 {
		return errors.New("incorrect number of batch_spec_workspaces updated")
	}

	return nil
}

const setChangesetSpecIDsOnBatchSpecWorkspaceQueryFmtstr = `
-- source: enterprise/internal/batches/store/worker_workspace_execution.go:setChangesetSpecIDs
UPDATE
	batch_spec_workspaces
SET
	changeset_spec_ids = %s
WHERE id = %s
`

// storeCacheResults builds DB cache entries for all the results and store them using the given tx.
func storeCacheResults(ctx context.Context, tx *Store, results []*batcheslib.CacheAfterStepResultMetadata, userID int32) error {
	for _, result := range results {
		value, err := json.Marshal(&result.Value)
		if err != nil {
			return errors.Wrap(err, "failed to marshal cache entry")
		}
		entry := &btypes.BatchSpecExecutionCacheEntry{
			Key:    result.Key,
			Value:  string(value),
			UserID: userID,
		}

		if err := tx.CreateBatchSpecExecutionCacheEntry(ctx, entry); err != nil {
			return errors.Wrap(err, "failed to save cache entry")
		}
	}

	return nil
}

func extractCacheEntries(events []*batcheslib.LogEvent) (cacheEntries []*batcheslib.CacheAfterStepResultMetadata, err error) {
	for _, e := range events {
		if e.Operation == batcheslib.LogEventOperationCacheAfterStepResult {
			m, ok := e.Metadata.(*batcheslib.CacheAfterStepResultMetadata)
			if !ok {
				return nil, errors.Newf("invalid log data, expected *batcheslib.CacheAfterStepResultMetadata got %T", e.Metadata)
			}

			cacheEntries = append(cacheEntries, m)
		}
	}

	return cacheEntries, nil
}

var ErrNoSrcCLILogEntry = errors.New("no src-cli log entry found in execution logs")

func logEventsFromLogEntries(logs []workerutil.ExecutionLogEntry) ([]*batcheslib.LogEvent, error) {
	if len(logs) < 1 {
		return nil, errors.Newf("job has no execution logs")
	}

	var (
		entry workerutil.ExecutionLogEntry
		found bool
	)

	for _, e := range logs {
		if e.Key == "step.src.0" {
			entry = e
			found = true
			break
		}
	}
	if !found {
		return nil, ErrNoSrcCLILogEntry
	}

	return btypes.ParseJSONLogsFromOutput(entry.Out), nil
}
