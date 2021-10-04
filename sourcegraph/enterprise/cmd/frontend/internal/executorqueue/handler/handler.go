package handler

import (
	"context"
	"fmt"

	"github.com/cockroachdb/errors"
	"github.com/inconshreveable/log15"

	apiclient "github.com/sourcegraph/sourcegraph/enterprise/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

type handler struct {
	QueueOptions
}

type QueueOptions struct {
	// Store is a required dbworker store store for each registered queue.
	Store store.Store

	// RecordTransformer is a required hook for each registered queue that transforms a generic
	// record from that queue into the job to be given to an executor.
	RecordTransformer func(ctx context.Context, record workerutil.Record) (apiclient.Job, error)

	// CanceledRecordsFetcher is an optional hook that can be provided to support cancelation.
	// If it is set, it will be invoked periodically and should return the IDs to be
	// canceled for the given executor.
	CanceledRecordsFetcher func(ctx context.Context, executorName string) (canceledIDs []int, err error)
}

func newHandler(queueOptions QueueOptions) *handler {
	return &handler{queueOptions}
}

var ErrUnknownJob = errors.New("unknown job")

// dequeue selects a job record from the database and stashes metadata including
// the job record and the locking transaction. If no job is available for processing,
// a false-valued flag is returned.
func (h *handler) dequeue(ctx context.Context, executorName, executorHostname string) (_ apiclient.Job, dequeued bool, _ error) {
	// We explicitly DON'T want to use executorHostname here, it is NOT guaranteed to be unique.
	record, dequeued, err := h.Store.Dequeue(ctx, executorName, nil)
	if err != nil {
		return apiclient.Job{}, false, err
	}
	if !dequeued {
		return apiclient.Job{}, false, nil
	}

	job, err := h.RecordTransformer(ctx, record)
	if err != nil {
		if _, err := h.Store.MarkFailed(ctx, record.RecordID(), fmt.Sprintf("failed to transform record: %s", err), store.MarkFinalOptions{}); err != nil {
			log15.Error("Failed to mark record as failed", "recordID", record.RecordID(), "error", err)
		}

		return apiclient.Job{}, false, err
	}

	return job, true, nil
}

// addExecutionLogEntry calls AddExecutionLogEntry for the given job.
func (h *handler) addExecutionLogEntry(ctx context.Context, executorName string, jobID int, entry workerutil.ExecutionLogEntry) (entryID int, err error) {
	entryID, err = h.Store.AddExecutionLogEntry(ctx, jobID, entry, store.ExecutionLogEntryOptions{
		// We pass the WorkerHostname, so the store enforces the record to be owned by this executor. When
		// the previous executor didn't report heartbeats anymore, but is still alive and reporting logs,
		// both executors that ever got the job would be writing to the same record. This prevents it.
		WorkerHostname: executorName,
		// We pass state to enforce adding log entries is only possible while the record is still dequeued.
		State: "processing",
	})
	if err == store.ErrExecutionLogEntryNotUpdated {
		return 0, ErrUnknownJob
	}
	return entryID, err
}

// updateExecutionLogEntry calls UpdateExecutionLogEntry for the given job and entry.
func (h *handler) updateExecutionLogEntry(ctx context.Context, executorName string, jobID int, entryID int, entry workerutil.ExecutionLogEntry) error {
	err := h.Store.UpdateExecutionLogEntry(ctx, jobID, entryID, entry, store.ExecutionLogEntryOptions{
		// We pass the WorkerHostname, so the store enforces the record to be owned by this executor. When
		// the previous executor didn't report heartbeats anymore, but is still alive and reporting logs,
		// both executors that ever got the job would be writing to the same record. This prevents it.
		WorkerHostname: executorName,
		// We pass state to enforce adding log entries is only possible while the record is still dequeued.
		State: "processing",
	})
	if err == store.ErrExecutionLogEntryNotUpdated {
		return ErrUnknownJob
	}
	return err
}

// markComplete calls MarkComplete for the given job.
func (h *handler) markComplete(ctx context.Context, executorName string, jobID int) error {
	ok, err := h.Store.MarkComplete(ctx, jobID, store.MarkFinalOptions{
		// We pass the WorkerHostname, so the store enforces the record to be owned by this executor. When
		// the previous executor didn't report heartbeats anymore, but is still alive and reporting state,
		// both executors that ever got the job would be writing to the same record. This prevents it.
		WorkerHostname: executorName,
	})
	if err != nil {
		return err
	}
	if !ok {
		return ErrUnknownJob
	}
	return nil
}

// markErrored calls MarkErrored for the given job.
func (h *handler) markErrored(ctx context.Context, executorName string, jobID int, errorMessage string) error {
	ok, err := h.Store.MarkErrored(ctx, jobID, errorMessage, store.MarkFinalOptions{
		// We pass the WorkerHostname, so the store enforces the record to be owned by this executor. When
		// the previous executor didn't report heartbeats anymore, but is still alive and reporting state,
		// both executors that ever got the job would be writing to the same record. This prevents it.
		WorkerHostname: executorName,
	})
	if err != nil {
		return err
	}
	if !ok {
		return ErrUnknownJob
	}
	return nil
}

// markFailed calls MarkFailed for the given job.
func (h *handler) markFailed(ctx context.Context, executorName string, jobID int, errorMessage string) error {
	ok, err := h.Store.MarkFailed(ctx, jobID, errorMessage, store.MarkFinalOptions{
		// We pass the WorkerHostname, so the store enforces the record to be owned by this executor. When
		// the previous executor didn't report heartbeats anymore, but is still alive and reporting state,
		// both executors that ever got the job would be writing to the same record. This prevents it.
		WorkerHostname: executorName,
	})
	if err != nil {
		return err
	}
	if !ok {
		return ErrUnknownJob
	}
	return nil
}

// heartbeat calls Heartbeat for the given jobs.
func (h *handler) heartbeat(ctx context.Context, executorName string, ids []int) (knownIDs []int, err error) {
	return h.Store.Heartbeat(ctx, ids, store.HeartbeatOptions{
		// We pass the WorkerHostname, so the store enforces the record to be owned by this executor. When
		// the previous executor didn't report heartbeats anymore, but is still alive and reporting state,
		// both executors that ever got the job would be writing to the same record. This prevents it.
		WorkerHostname: executorName,
	})
}

// canceled reaches to the queueOptions.FetchCanceled to determine jobs that need
// to be canceled.
func (h *handler) canceled(ctx context.Context, executorName string) (knownIDs []int, err error) {
	if h.CanceledRecordsFetcher == nil {
		return nil, nil
	}
	return h.CanceledRecordsFetcher(ctx, executorName)
}
