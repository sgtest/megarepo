package handler

import (
	"context"
	"fmt"
	"math/rand"
	"net/http"
	"strings"
	"time"

	"github.com/sourcegraph/log"
	"golang.org/x/exp/slices"

	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	executorstore "github.com/sourcegraph/sourcegraph/enterprise/internal/executor/store"
	executortypes "github.com/sourcegraph/sourcegraph/enterprise/internal/executor/types"
	"github.com/sourcegraph/sourcegraph/internal/database"
	metricsstore "github.com/sourcegraph/sourcegraph/internal/metrics/store"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/api"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// MultiHandler handles the HTTP requests of an executor for more than one queue. See ExecutorHandler for single-queue implementation.
type MultiHandler struct {
	executorStore         database.ExecutorStore
	JobTokenStore         executorstore.JobTokenStore
	metricsStore          metricsstore.DistributedStore
	CodeIntelQueueHandler QueueHandler[uploadsshared.Index]
	BatchesQueueHandler   QueueHandler[*btypes.BatchSpecWorkspaceExecutionJob]
	validQueues           []string
	RandomGenerator       RandomGenerator
	logger                log.Logger
}

// NewMultiHandler creates a new MultiHandler.
func NewMultiHandler(
	executorStore database.ExecutorStore,
	jobTokenStore executorstore.JobTokenStore,
	metricsStore metricsstore.DistributedStore,
	codeIntelQueueHandler QueueHandler[uploadsshared.Index],
	batchesQueueHandler QueueHandler[*btypes.BatchSpecWorkspaceExecutionJob],
) MultiHandler {
	return MultiHandler{
		executorStore:         executorStore,
		JobTokenStore:         jobTokenStore,
		metricsStore:          metricsStore,
		CodeIntelQueueHandler: codeIntelQueueHandler,
		BatchesQueueHandler:   batchesQueueHandler,
		validQueues:           []string{codeIntelQueueHandler.Name, batchesQueueHandler.Name},
		RandomGenerator:       &realRandom{},
		logger:                log.Scoped("executor-multi-queue-handler", "The route handler for all executor queues"),
	}
}

// HandleDequeue is the equivalent of ExecutorHandler.HandleDequeue for multiple queues.
func (m *MultiHandler) HandleDequeue(w http.ResponseWriter, r *http.Request) {
	var payload executortypes.DequeueRequest
	wrapHandler(w, r, &payload, m.logger, func() (int, any, error) {
		job, dequeued, err := m.dequeue(r.Context(), payload)
		if !dequeued {
			return http.StatusNoContent, nil, err
		}

		return http.StatusOK, job, err
	})
}

func (m *MultiHandler) dequeue(ctx context.Context, req executortypes.DequeueRequest) (executortypes.Job, bool, error) {
	if err := validateWorkerHostname(req.ExecutorName); err != nil {
		m.logger.Error(err.Error())
		return executortypes.Job{}, false, err
	}

	version2Supported := false
	if req.Version != "" {
		var err error
		version2Supported, err = api.CheckSourcegraphVersion(req.Version, "4.3.0-0", "2022-11-24")
		if err != nil {
			return executortypes.Job{}, false, err
		}
	}

	if len(req.Queues) == 0 {
		m.logger.Info("Dequeue requested without any queue names", log.String("executorName", req.ExecutorName))
		return executortypes.Job{}, false, nil
	}

	if invalidQueues := m.validateQueues(req.Queues); len(invalidQueues) > 0 {
		message := fmt.Sprintf("Invalid queue name(s) '%s' found. Supported queue names are '%s'.", strings.Join(invalidQueues, ", "), strings.Join(m.validQueues, ", "))
		m.logger.Error(message)
		return executortypes.Job{}, false, errors.New(message)
	}

	resourceMetadata := ResourceMetadata{
		NumCPUs:   req.NumCPUs,
		Memory:    req.Memory,
		DiskSpace: req.DiskSpace,
	}

	// Initialize the random number generator
	m.RandomGenerator.Seed(time.Now().UnixNano())

	// Shuffle the slice using the Fisher-Yates algorithm
	for i := len(req.Queues) - 1; i > 0; i-- {
		j := m.RandomGenerator.Intn(i + 1)
		req.Queues[i], req.Queues[j] = req.Queues[j], req.Queues[i]
	}

	logger := m.logger.Scoped("dequeue", "Pick a job record from the database.")
	var job executortypes.Job
	for _, queue := range req.Queues {
		switch queue {
		case m.BatchesQueueHandler.Name:
			record, dequeued, err := m.BatchesQueueHandler.Store.Dequeue(ctx, req.ExecutorName, nil)
			if err != nil {
				err = errors.Wrapf(err, "dbworkerstore.Dequeue %s", queue)
				logger.Error("Failed to dequeue", log.String("queue", queue), log.Error(err))
				return executortypes.Job{}, false, err
			}
			if !dequeued {
				// no batches job to dequeue, try next queue
				continue
			}

			job, err = m.BatchesQueueHandler.RecordTransformer(ctx, req.Version, record, resourceMetadata)
			if err != nil {
				markErr := markRecordAsFailed(ctx, m.BatchesQueueHandler.Store, record.RecordID(), err, logger)
				err = errors.Wrapf(errors.Append(err, markErr), "RecordTransformer %s", queue)
				logger.Error("Failed to transform record", log.String("queue", queue), log.Error(err))
				return executortypes.Job{}, false, err
			}
		case m.CodeIntelQueueHandler.Name:
			record, dequeued, err := m.CodeIntelQueueHandler.Store.Dequeue(ctx, req.ExecutorName, nil)
			if err != nil {
				err = errors.Wrapf(err, "dbworkerstore.Dequeue %s", queue)
				logger.Error("Failed to dequeue", log.String("queue", queue), log.Error(err))
				return executortypes.Job{}, false, err
			}
			if !dequeued {
				// no codeintel job to dequeue, try next queue
				continue
			}

			job, err = m.CodeIntelQueueHandler.RecordTransformer(ctx, req.Version, record, resourceMetadata)
			if err != nil {
				markErr := markRecordAsFailed(ctx, m.CodeIntelQueueHandler.Store, record.RecordID(), err, logger)
				err = errors.Wrapf(errors.Append(err, markErr), "RecordTransformer %s", queue)
				logger.Error("Failed to transform record", log.String("queue", queue), log.Error(err))
				return executortypes.Job{}, false, err
			}
		}
		if job.ID != 0 {
			job.Queue = queue
			break
		}
	}

	if job.ID == 0 {
		// all queues are empty, return nothing
		return executortypes.Job{}, false, nil
	}

	// If this executor supports v2, return a v2 payload. Based on this field,
	// marshalling will be switched between old and new payload.
	if version2Supported {
		job.Version = 2
	}

	logger = m.logger.Scoped("token", "Create or regenerate a job token.")
	token, err := m.JobTokenStore.Create(ctx, job.ID, job.Queue, job.RepositoryName)
	if err != nil {
		if errors.Is(err, executorstore.ErrJobTokenAlreadyCreated) {
			// Token has already been created, regen it.
			token, err = m.JobTokenStore.Regenerate(ctx, job.ID, job.Queue)
			if err != nil {
				err = errors.Wrap(err, "RegenerateToken")
				logger.Error("Failed to regenerate token", log.Error(err))
				return executortypes.Job{}, false, err
			}
		} else {
			err = errors.Wrap(err, "CreateToken")
			logger.Error("Failed to create token", log.Error(err))
			return executortypes.Job{}, false, err
		}
	}
	job.Token = token

	return job, true, nil
}

// HandleHeartbeat processes a heartbeat from a multi-queue executor.
func (m *MultiHandler) HandleHeartbeat(w http.ResponseWriter, r *http.Request) {
	var payload executortypes.HeartbeatRequest

	wrapHandler(w, r, &payload, m.logger, func() (int, any, error) {
		e := types.Executor{
			Hostname:        payload.ExecutorName,
			QueueNames:      payload.QueueNames,
			OS:              payload.OS,
			Architecture:    payload.Architecture,
			DockerVersion:   payload.DockerVersion,
			ExecutorVersion: payload.ExecutorVersion,
			GitVersion:      payload.GitVersion,
			IgniteVersion:   payload.IgniteVersion,
			SrcCliVersion:   payload.SrcCliVersion,
		}

		// Handle metrics in the background, this should not delay the heartbeat response being
		// delivered. It is critical for keeping jobs alive.
		go func() {
			metrics, err := decodeAndLabelMetrics(payload.PrometheusMetrics, payload.ExecutorName)
			if err != nil {
				// Just log the error but don't panic. The heartbeat is more important.
				m.logger.Error("failed to decode metrics and apply labels for executor heartbeat", log.Error(err))
				return
			}

			if err = m.metricsStore.Ingest(payload.ExecutorName, metrics); err != nil {
				// Just log the error but don't panic. The heartbeat is more important.
				m.logger.Error("failed to ingest metrics for executor heartbeat", log.Error(err))
			}
		}()

		knownIDs, cancelIDs, err := m.heartbeat(r.Context(), e, payload.JobIDsByQueue)

		return http.StatusOK, executortypes.HeartbeatResponse{KnownIDs: knownIDs, CancelIDs: cancelIDs}, err
	})
}

func (m *MultiHandler) heartbeat(ctx context.Context, executor types.Executor, idsByQueue []executortypes.QueueJobIDs) (knownIDs, cancelIDs []string, err error) {
	if err = validateWorkerHostname(executor.Hostname); err != nil {
		return nil, nil, err
	}

	if len(executor.QueueNames) == 0 {
		return nil, nil, errors.Newf("queueNames must be set for multi-queue heartbeats")
	}

	var invalidQueueNames []string
	for _, queue := range idsByQueue {
		if !slices.Contains(executor.QueueNames, queue.QueueName) {
			invalidQueueNames = append(invalidQueueNames, queue.QueueName)
		}
	}
	if len(invalidQueueNames) > 0 {
		return nil, nil, errors.Newf(
			"unsupported queue name(s) '%s' submitted in queueJobIds, executor is configured for queues '%s'",
			strings.Join(invalidQueueNames, ", "),
			strings.Join(executor.QueueNames, ", "),
		)
	}

	logger := log.Scoped("multiqueue.heartbeat", "Write the heartbeat of multiple queues to the database")

	// Write this heartbeat to the database so that we can populate the UI with recent executor activity.
	if err = m.executorStore.UpsertHeartbeat(ctx, executor); err != nil {
		logger.Error("Failed to upsert executor heartbeat", log.Error(err), log.Strings("queues", executor.QueueNames))
	}

	for _, queue := range idsByQueue {
		heartbeatOptions := dbworkerstore.HeartbeatOptions{
			// see handler.heartbeat for explanation of this field
			WorkerHostname: executor.Hostname,
		}

		var known []string
		var cancel []string

		switch queue.QueueName {
		case m.BatchesQueueHandler.Name:
			known, cancel, err = m.BatchesQueueHandler.Store.Heartbeat(ctx, queue.JobIDs, heartbeatOptions)
		case m.CodeIntelQueueHandler.Name:
			known, cancel, err = m.CodeIntelQueueHandler.Store.Heartbeat(ctx, queue.JobIDs, heartbeatOptions)
		}

		if err != nil {
			return nil, nil, errors.Wrap(err, "multiqueue.UpsertHeartbeat")
		}

		// TODO: this could move into the executor client's Heartbeat impl, but considering this is
		// multi-queue specific code, it's a bit ambiguous where it should live. Having it here allows
		// types.HeartbeatResponse to be simpler and enables the client to pass the ID sets back to the worker
		// without further single/multi queue logic
		for i, knownID := range known {
			known[i] = knownID + "-" + queue.QueueName
		}
		for i, cancelID := range cancel {
			cancel[i] = cancelID + "-" + queue.QueueName
		}
		knownIDs = append(knownIDs, known...)
		cancelIDs = append(cancelIDs, cancel...)
	}

	return knownIDs, cancelIDs, nil
}

func (m *MultiHandler) validateQueues(queues []string) []string {
	var invalidQueues []string
	for _, queue := range queues {
		if !slices.Contains(m.validQueues, queue) {
			invalidQueues = append(invalidQueues, queue)
		}
	}
	return invalidQueues
}

func markRecordAsFailed[T workerutil.Record](context context.Context, store dbworkerstore.Store[T], recordID int, err error, logger log.Logger) error {
	_, markErr := store.MarkFailed(context, recordID, fmt.Sprintf("failed to transform record: %s", err), dbworkerstore.MarkFinalOptions{})
	if markErr != nil {
		logger.Error("Failed to mark record as failed",
			log.Int("recordID", recordID),
			log.Error(markErr))
	}
	return markErr
}

// RandomGenerator is a wrapper for generating random numbers to support simple queue fairness.
// Its functions can be mocked out for consistent dequeuing in unit tests.
type RandomGenerator interface {
	Seed(seed int64)
	Intn(n int) int
}

type realRandom struct{}

func (r *realRandom) Seed(seed int64) {
	rand.Seed(seed)
}

func (r *realRandom) Intn(n int) int {
	return rand.Intn(n)
}
