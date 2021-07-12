package background

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/cockroachdb/errors"
	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

// executorStalledJobMaximumAge is the maximum allowable duration between updating the state of a
// job as "processing" and locking the record during processing. An unlocked row that is
// marked as processing likely indicates that the executor that dequeued the job has died.
// There should be a nearly-zero delay between these states during normal operation.
const executorStalledJobMaximumAge = time.Second * 5

// executorMaximumNumResets is the maximum number of times a job can be reset. If a job's failed
// attempts counter reaches this threshold, it will be moved into "errored" rather than
// "queued" on its next reset.
const executorMaximumNumResets = 3

var executorWorkerStoreOptions = dbworkerstore.Options{
	Name:              "batch_spec_executor_worker_store",
	TableName:         "batch_spec_executions",
	ColumnExpressions: store.BatchSpecExecutionColumns,
	Scan:              scanFirstExecutionRecord,
	OrderByExpression: sqlf.Sprintf("batch_spec_executions.created_at, batch_spec_executions.id"),
	StalledMaxAge:     executorStalledJobMaximumAge,
	MaxNumResets:      executorMaximumNumResets,
	// Explicitly disable retries.
	MaxNumRetries: 0,
}

// NewExecutorStore creates a dbworker store that wraps the batch_spec_executions
// table.
func NewExecutorStore(s basestore.ShareableStore, observationContext *observation.Context) dbworkerstore.Store {
	return &executorStore{Store: dbworkerstore.NewWithMetrics(s.Handle(), executorWorkerStoreOptions, observationContext)}
}

var _ dbworkerstore.Store = &executorStore{}

// executorStore is a thin wrapper around dbworkerstore.Store that allows us to
// extract information out of the ExecutionLogEntry field and persisting it to
// separate columns when marking a job as complete.
type executorStore struct {
	dbworkerstore.Store
}

// markCompleteQuery is taken from internal/workerutil/dbworker/store/store.go
//
// If that one changes we need to update this one here too.
const markCompleteQuery = `
UPDATE batch_spec_executions
SET state = 'completed', finished_at = clock_timestamp(), batch_spec_id = (SELECT id FROM batch_specs WHERE rand_id = %s)
WHERE id = %s AND state = 'processing'
RETURNING id
`

func (s *executorStore) MarkComplete(ctx context.Context, id int) (_ bool, err error) {
	batchesStore := store.New(s.Store.Handle().DB(), nil)

	batchSpecRandID, err := loadAndExtractBatchSpecRandID(ctx, batchesStore, int64(id))
	if err != nil {
		// If we couldn't extract the batch spec rand id, we mark the job as failed
		return s.Store.MarkFailed(ctx, id, fmt.Sprintf("failed to extract batch spec ID: %s", err))
	}

	_, ok, err := basestore.ScanFirstInt(batchesStore.Query(ctx, sqlf.Sprintf(markCompleteQuery, batchSpecRandID, id)))

	return ok, err
}

func (s *executorStore) DequeueWithIndependentTransactionContext(ctx context.Context, workerHostname string, conditions []*sqlf.Query) (workerutil.Record, dbworkerstore.Store, bool, error) {
	r, wrapped, b, err := s.Store.DequeueWithIndependentTransactionContext(ctx, workerHostname, conditions)

	return r, &executorStore{Store: wrapped}, b, err
}

func (s *executorStore) Dequeue(ctx context.Context, workerHostname string, conditions []*sqlf.Query) (workerutil.Record, dbworkerstore.Store, bool, error) {
	r, wrapped, b, err := s.Store.Dequeue(ctx, workerHostname, conditions)

	return r, &executorStore{Store: wrapped}, b, err
}

func loadAndExtractBatchSpecRandID(ctx context.Context, s *store.Store, id int64) (string, error) {
	exec, err := s.GetBatchSpecExecution(ctx, store.GetBatchSpecExecutionOpts{ID: id})
	if err != nil {
		return "", err
	}

	if len(exec.ExecutionLogs) < 1 {
		return "", errors.New("no execution logs")
	}

	return extractBatchSpecRandID(exec.ExecutionLogs)
}

var ErrNoBatchSpecRandID = errors.New("no batch spec rand id found in execution logs")

func extractBatchSpecRandID(logs []workerutil.ExecutionLogEntry) (string, error) {
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
		return "", ErrNoBatchSpecRandID
	}

	var batchSpecRandID string
	for _, l := range strings.Split(entry.Out, "\n") {
		const outputLinePrefix = "stdout: "

		if !strings.HasPrefix(l, outputLinePrefix) {
			continue
		}

		jsonPart := l[len(outputLinePrefix):]

		var e srcCLILogLine
		if err := json.Unmarshal([]byte(jsonPart), &e); err != nil {
			// If we can't unmarshal the line as JSON we skip it
			continue
		}

		if e.Operation == operationCreatingBatchSpec && e.Status == "SUCCESS" {
			parts := strings.Split(e.Message, "/")
			if len(parts) == 0 {
				return "", ErrNoBatchSpecRandID
			}

			batchSpecGraphQLID := graphql.ID(parts[len(parts)-1])
			if err := relay.UnmarshalSpec(batchSpecGraphQLID, &batchSpecRandID); err != nil {
				// If we can't extract the ID we simply return our main error
				return "", ErrNoBatchSpecRandID
			}

			return batchSpecRandID, nil
		}
	}

	return batchSpecRandID, ErrNoBatchSpecRandID
}

// srcCLILogLine matches the definition of log entries that are printed by
// src-cli when used with the `-text-only` flag.
type srcCLILogLine struct {
	Operation string `json:"operation"` // "PREPARING_DOCKER_IMAGES"

	Timestamp time.Time `json:"timestamp"`

	Status  string `json:"status"`            // "STARTED", "PROGRESS", "SUCCESS", "FAILURE"
	Message string `json:"message,omitempty"` // "70% done"
}

const operationCreatingBatchSpec = "CREATING_BATCH_SPEC"

// scanFirstExecutionRecord scans a slice of batch change executions and returns the first.
func scanFirstExecutionRecord(rows *sql.Rows, err error) (workerutil.Record, bool, error) {
	return store.ScanFirstBatchSpecExecution(rows, err)
}
