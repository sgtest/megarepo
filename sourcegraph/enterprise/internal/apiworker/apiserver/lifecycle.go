package apiserver

import (
	"context"
	"errors"
	"fmt"

	"github.com/hashicorp/go-multierror"
	"github.com/inconshreveable/log15"
)

var shutdownErr = errors.New("server shutting down")

// heartbeat will release the transaction for any job that is not confirmed to be in-progress
// by the given executor. This method is called when the executor POSTs its in-progress job
// identifiers to the /heartbeat route.
func (h *handler) heartbeat(ctx context.Context, executorName string, jobIDs []int) error {
	return h.requeueJobs(ctx, h.pruneJobs(executorName, jobIDs))
}

// cleanup will release the transactions held by any executor that has not sent a heartbeat
// in a while. This method is called periodically in the background.
func (h *handler) cleanup(ctx context.Context) error {
	return h.requeueJobs(ctx, h.pruneExecutors())
}

// shutdown releases all transactions. This method is called on process shutdown.
func (h *handler) shutdown() {
	h.m.Lock()
	defer h.m.Unlock()

	for _, executor := range h.executors {
		for _, job := range executor.jobs {
			if err := job.tx.Done(shutdownErr); err != nil && err != shutdownErr {
				log15.Error(fmt.Sprintf("Failed to close transaction holding job %d", job.record.RecordID()), "err", err)
			}
		}
	}
}

// pruneJobs updates the set of job identifiers assigned to the given executor and returns
// any job that was known to us but not reported by the executor.
func (h *handler) pruneJobs(executorName string, ids []int) (dead []jobMeta) {
	now := h.clock.Now()

	idMap := map[int]struct{}{}
	for _, id := range ids {
		idMap[id] = struct{}{}
	}

	h.m.Lock()
	defer h.m.Unlock()

	executor, ok := h.executors[executorName]
	if !ok {
		executor = &executorMeta{}
		h.executors[executorName] = executor
	}

	var live []jobMeta
	for _, job := range executor.jobs {
		if _, ok := idMap[job.record.RecordID()]; ok || now.Sub(job.started) < h.options.UnreportedMaxAge {
			live = append(live, job)
		} else {
			dead = append(dead, job)
		}
	}

	executor.jobs = live
	executor.lastUpdate = now
	return dead
}

// pruneExecutors will release the transactions held by any executor that has not sent a
// heartbeat in a while and return the attached jobs.
func (h *handler) pruneExecutors() (jobs []jobMeta) {
	h.m.Lock()
	defer h.m.Unlock()

	for name, executor := range h.executors {
		if h.clock.Now().Sub(executor.lastUpdate) <= h.options.DeathThreshold {
			continue
		}

		jobs = append(jobs, executor.jobs...)
		delete(h.executors, name)
	}

	return jobs
}

// requeueJobs releases and requeues each of the given jobs.
func (h *handler) requeueJobs(ctx context.Context, jobs []jobMeta) (errs error) {
	for _, job := range jobs {
		if err := h.requeueJob(ctx, job); err != nil {
			errs = multierror.Append(errs, err)
		}
	}

	return errs
}

// requeueJob requeues the given job and releases the associated transaction.
func (h *handler) requeueJob(ctx context.Context, job jobMeta) error {
	defer func() { h.dequeueSemaphore <- struct{}{} }()

	err := job.tx.Requeue(ctx, job.record.RecordID(), h.clock.Now().Add(h.options.RequeueDelay))
	return job.tx.Done(err)
}
