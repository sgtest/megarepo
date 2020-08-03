package store

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/sourcegraph/internal/db/basestore"
)

// Store is the database layer for the workerutil package that handles worker-side operations.
type Store interface {
	basestore.ShareableStore

	// Done performs a commit or rollback of the underlying transaction/savepoint depending
	// returned from the Dequeue method. See basestore.Store#Done for additional documentation.
	Done(err error) error

	// Dequeue selects the first unlocked record matching the given conditions and locks it in a new transaction that
	// should be held by the worker process. If there is such a record, it is returned along with a new store instance
	// that wraps the transaction. The resulting transaction must be closed by the caller, and the transaction should
	// include a state transition of the record into a terminal state. If there is no such unlocked record, a nil record
	// and a nil store will be returned along with a false-valued flag. This method must not be called from within a
	// transaction.
	//
	// The supplied conditions may use the alias provided in `ViewName`, if one was supplied.
	Dequeue(ctx context.Context, conditions []*sqlf.Query) (record Record, tx Store, exists bool, err error)

	// DequeueWithIndependentTransactionContext is like Dequeue, but will use a context.Background() for the underlying
	// transaction context. This method allows the transaction to lexically outlive the code in which it was created. This
	// is useful if a longer-running transaction is managed explicitly bewteen multiple goroutines.
	DequeueWithIndependentTransactionContext(ctx context.Context, conditions []*sqlf.Query) (Record, Store, bool, error)

	// Requeue updates the state of the record with the given identifier to queued and adds a processing delay before
	// the next dequeue of this record can be performed.
	Requeue(ctx context.Context, id int, after time.Time) error

	// MarkComplete attempts to update the state of the record to complete. If this record has already been moved from
	// the processing state to a terminal state, this method will have no effect. This method returns a boolean flag
	// indicating if the record was updated.
	MarkComplete(ctx context.Context, id int) (bool, error)

	// MarkErrored attempts to update the state of the record to errored. This method will only have an effect
	// if the current state of the record is processing or completed. A requeued record or a record already marked
	// with an error will not be updated. This method returns a boolean flag indicating if the record was updated.
	MarkErrored(ctx context.Context, id int, failureMessage string) (bool, error)

	// ResetStalled moves all unlocked records in the processing state for more than `StalledMaxAge` back to the queued
	// state. In order to prevent input that continually crashes worker instances, records that have been reset more
	// than `MaxNumResets` times will be marked as errored. This method returns a list of record identifiers that have
	// been reset and a list of record identifiers that have been marked as errored.
	ResetStalled(ctx context.Context) (resetIDs, erroredIDs []int, err error)
}

// Record is a generic interface for record conforming to the requirements of the store.
type Record interface {
	// RecordID returns the integer primary key of the record.
	RecordID() int
}

type store struct {
	*basestore.Store
	options        StoreOptions
	columnReplacer *strings.Replacer
}

var _ Store = &store{}

// StoreOptions configure the behavior of Store over a particular set of tables, columns, and expressions.
type StoreOptions struct {
	// TableName is the name of the table containing work records.
	//
	// The target table (and the target view referenced by `ViewName`) must have the following columns
	// and types:
	//
	//   - id: integer primary key
	//   - state: an enum type containing at least `queued`, `processing`, and `errored`
	//   - failure_message: text
	//   - started_at: timestamp with time zone
	//   - finished_at: timestamp with time zone
	//   - process_after: timestamp with time zone
	//   - num_resets: integer not null
	//
	// The names of these columns may be customized based on the table name by adding a replacement
	// pair in the AlternateColumnNames mapping.
	//
	// It's recommended to put an index or (or partial index) on the state column for more efficient
	// dequeue operations.
	TableName string

	// AlternateColumnNames is a map from expected column names to actual column names in the target
	// table. This allows existing tables to be more easily retrofitted into the expected record
	// shape.
	AlternateColumnNames map[string]string

	// ViewName is an optional name of a view on top of the table containing work records to query when
	// selecting a candidate and when selecting the record after it has been locked. If this value is
	// not supplied, `TableName` will be used. The value supplied may also indicate a table alias, which
	// can be referenced in `OrderByExpression`, `ColumnExpressions`, and the conditions suplied to
	// `Dequeue`.
	//
	// The target of this column must be a view on top of the configured table with the same column
	// requirements as the base table descried above.
	//
	// Example use case:
	// The processor for LSIF uploads supplies `lsif_uploads_with_repository_name`, a view on top of the
	// `lsif_uploads` table that joins work records with the `repo` table and adds an additional repository
	// name column. This allows `Dequeue` to return a record with additional data so that a second query
	// is not necessary by the caller.
	ViewName string

	// Scan is the function used to convert a rows object into a record of the expected shape.
	Scan RecordScanFn

	// OrderByExpression is the SQL expression used to order candidate records when selecting the next
	// batch of work to perform. This expression may use the alias provided in `ViewName`, if one was
	// supplied.
	OrderByExpression *sqlf.Query

	// ColumnExpressions are the target columns provided to the query when selecting a locked record.
	// These expressions may use the alias provided in `ViewName`, if one was supplied.
	ColumnExpressions []*sqlf.Query

	// StalledMaxAge is the maximum allow duration between updating the state of a record as "processing"
	// and locking the record row during processing. An unlocked row that is marked as processing likely
	// indicates that the worker that dequeued the record has died. There should be a nearly-zero delay
	// between these states during normal operation.
	StalledMaxAge time.Duration

	// MaxNumResets is the maximum number of times a record can be implicitly reset back to the queued
	// state (via `ResetStalled`). If a record's failed attempts counter reaches this threshold, it will
	// be moved into the errored state rather than queued on its next reset to prevent an infinite retry
	// cycle of the same input.
	MaxNumResets int
}

// RecordScanFn is a function that interprets row values as a particular record. This function should
// return a false-valued flag if the given result set was empty. This function must close the rows
// value if the given error value is nil.
//
// See the `CloseRows` function in the store/base package for suggested implementation details.
type RecordScanFn func(rows *sql.Rows, err error) (Record, bool, error)

// NewStore creates a new store with the given database handle and options.
func NewStore(handle *basestore.TransactableHandle, options StoreOptions) Store {
	return newStore(handle, options)
}

// newStore creates a new store with the given database handle and options.
func newStore(handle *basestore.TransactableHandle, options StoreOptions) *store {
	if options.ViewName == "" {
		options.ViewName = options.TableName
	}

	alternateColumnNames := map[string]string{}
	for _, name := range columnNames {
		alternateColumnNames[name] = name
	}
	for k, v := range options.AlternateColumnNames {
		alternateColumnNames[k] = v
	}

	var replacements []string
	for k, v := range alternateColumnNames {
		replacements = append(replacements, fmt.Sprintf("{%s}", k), v)
	}

	return &store{
		Store:          basestore.NewWithHandle(handle),
		options:        options,
		columnReplacer: strings.NewReplacer(replacements...),
	}
}

// ColumnNames are the names of the columns expected to be defined by the target table.
var columnNames = []string{
	"id",
	"state",
	"failure_message",
	"started_at",
	"finished_at",
	"process_after",
	"num_resets",
}

func (s *store) Transact(ctx context.Context) (*store, error) {
	txBase, err := s.Store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return &store{Store: txBase, options: s.options, columnReplacer: s.columnReplacer}, nil
}

// Dequeue selects the first unlocked record matching the given conditions and locks it in a new transaction that
// should be held by the worker process. If there is such a record, it is returned along with a new store instance
// that wraps the transaction. The resulting transaction must be closed by the caller, and the transaction should
// include a state transition of the record into a terminal state. If there is no such unlocked record, a nil record
// and a nil store will be returned along with a false-valued flag. This method must not be called from within a
// transaction.
//
// The supplied conditions may use the alias provided in `ViewName`, if one was supplied.
func (s *store) Dequeue(ctx context.Context, conditions []*sqlf.Query) (record Record, _ Store, exists bool, err error) {
	return s.dequeue(ctx, conditions, false)
}

// DequeueWithIndependentTransactionContext is like Dequeue, but will use a context.Background() for the underlying
// transaction context. This method allows the transaction to lexically outlive the code in which it was created. This
// is useful if a longer-running transaction is managed explicitly bewteen multiple goroutines.
func (s *store) DequeueWithIndependentTransactionContext(ctx context.Context, conditions []*sqlf.Query) (Record, Store, bool, error) {
	return s.dequeue(ctx, conditions, true)
}

func (s *store) dequeue(ctx context.Context, conditions []*sqlf.Query, independentTxCtx bool) (record Record, _ Store, exists bool, err error) {
	if s.InTransaction() {
		return nil, nil, false, ErrDequeueTransaction
	}

	query := s.formatQuery(
		selectCandidateQuery,
		quote(s.options.ViewName),
		makeConditionSuffix(conditions),
		s.options.OrderByExpression,
		quote(s.options.TableName),
	)

	for {
		// First, we try to select an eligible record outside of a transaction. This will skip
		// any rows that are currently locked inside of a transaction of another dequeue process.
		id, ok, err := basestore.ScanFirstInt(s.Query(ctx, query))
		if err != nil {
			return nil, nil, false, err
		}
		if !ok {
			return nil, nil, false, nil
		}

		// Once we have an eligible identifier, we try to create a transaction and select the
		// record in a way that takes a row lock for the duration of the transaction.
		tx, err := s.Transact(ctx)
		if err != nil {
			return nil, nil, false, err
		}

		// Select the candidate record within the transaction to lock it from other processes. Note
		// that SKIP LOCKED here is necessary, otherwise this query would block on race conditions
		// until the other process has finished with the record.
		_, exists, err = basestore.ScanFirstInt(tx.Query(ctx, s.formatQuery(
			lockQuery,
			quote(s.options.TableName),
			id,
		)))
		if err != nil {
			return nil, nil, false, tx.Done(err)
		}
		if !exists {
			// Due to SKIP LOCKED, This query will return a sql.ErrNoRows error if the record has
			// already been locked in another process's transaction. We'll return a special error
			// that is checked by the caller to try to select a different record.
			if err := tx.Done(ErrDequeueRace); err != ErrDequeueRace {
				return nil, nil, false, err
			}

			// This will occur if we selected a candidate record that raced with another dequeue
			// process. If both dequeue processes select the same record and the other process
			// begins its transaction first, this condition will occur. We'll re-try the process
			// by selecting another identifier - this one will be skipped on a second attempt as
			// it is now locked.
			continue

		}

		// The record is now locked in this transaction. As `TableName` and `ViewName` may have distinct
		// values, we need to perform a second select in order to pass the correct data to the scan
		// function.
		record, exists, err = s.options.Scan(tx.Query(ctx, s.formatQuery(
			selectRecordQuery,
			sqlf.Join(s.options.ColumnExpressions, ", "),
			quote(s.options.ViewName),
			id,
		)))
		if err != nil {
			return nil, nil, false, tx.Done(err)
		}
		if !exists {
			// This only happens on a programming error (mismatch between `TableName` and `ViewName`).
			return nil, nil, false, tx.Done(ErrNoRecord)
		}

		return record, tx, true, nil
	}
}

const selectCandidateQuery = `
-- source: internal/workerutil/store.go:Dequeue
WITH candidate AS (
	SELECT {id} FROM %s
	WHERE
		{state} = 'queued' AND
		({process_after} IS NULL OR {process_after} <= NOW())
		%s
	ORDER BY %s
	FOR UPDATE SKIP LOCKED
	LIMIT 1
)
UPDATE %s
SET
	{state} = 'processing',
	{started_at} = NOW()
WHERE {id} IN (SELECT {id} FROM candidate)
RETURNING {id}
`

const lockQuery = `
-- source: internal/workerutil/store.go:Dequeue
SELECT 1 FROM %s
WHERE {id} = %s
FOR UPDATE SKIP LOCKED
LIMIT 1
`

const selectRecordQuery = `
-- source: internal/workerutil/store.go:Dequeue
SELECT %s FROM %s
WHERE {id} = %s
LIMIT 1
`

// Requeue updates the state of the record with the given identifier to queued and adds a processing delay before
// the next dequeue of this record can be performed.
func (s *store) Requeue(ctx context.Context, id int, after time.Time) error {
	return s.Exec(ctx, s.formatQuery(
		requeueQuery,
		quote(s.options.TableName),
		after,
		id,
	))
}

const requeueQuery = `
-- source: internal/workerutil/store.go:Requeue
UPDATE %s
SET {state} = 'queued', {process_after} = %s
WHERE {id} = %s
`

// MarkComplete attempts to update the state of the record to complete. If this record has already been moved from
// the processing state to a terminal state, this method will have no effect. This method returns a boolean flag
// indicating if the record was updated.
func (s *store) MarkComplete(ctx context.Context, id int) (bool, error) {
	_, ok, err := basestore.ScanFirstInt(s.Query(ctx, s.formatQuery(markCompleteQuery, quote(s.options.TableName), id)))
	return ok, err
}

const markCompleteQuery = `
-- source: internal/workerutil/store.go:MarkComplete
UPDATE %s
SET {state} = 'completed', {finished_at} = clock_timestamp()
WHERE {id} = %s AND {state} = 'processing'
RETURNING {id}
`

// MarkErrored attempts to update the state of the record to errored. This method will only have an effect
// if the current state of the record is processing or completed. A requeued record or a record already marked
// with an error will not be updated. This method returns a boolean flag indicating if the record was updated.
func (s *store) MarkErrored(ctx context.Context, id int, failureMessage string) (bool, error) {
	_, ok, err := basestore.ScanFirstInt(s.Query(ctx, s.formatQuery(markErroredQuery, quote(s.options.TableName), failureMessage, id)))
	return ok, err
}

const markErroredQuery = `
-- source: internal/workerutil/store.go:MarkErrored
UPDATE %s
SET {state} = 'errored', {finished_at} = clock_timestamp(), {failure_message} = %s
WHERE {id} = %s AND ({state} = 'processing' OR {state} = 'completed')
RETURNING {id}
`

// ResetStalled moves all unlocked records in the processing state for more than `StalledMaxAge` back to the queued
// state. In order to prevent input that continually crashes worker instances, records that have been reset more
// than `MaxNumResets` times will be marked as errored. This method returns a list of record identifiers that have
// been reset and a list of record identifiers that have been marked as errored.
func (s *store) ResetStalled(ctx context.Context) (resetIDs, erroredIDs []int, err error) {
	resetIDs, err = s.resetStalled(ctx, resetStalledQuery)
	if err != nil {
		return resetIDs, erroredIDs, err
	}

	erroredIDs, err = s.resetStalled(ctx, resetStalledMaxResetsQuery)
	if err != nil {
		return resetIDs, erroredIDs, err
	}

	return resetIDs, erroredIDs, nil
}

func (s *store) resetStalled(ctx context.Context, q string) ([]int, error) {
	return basestore.ScanInts(s.Query(
		ctx,
		s.formatQuery(
			q,
			quote(s.options.TableName),
			int(s.options.StalledMaxAge/time.Second),
			s.options.MaxNumResets,
			quote(s.options.TableName),
		),
	))
}

const resetStalledQuery = `
-- source: internal/workerutil/store.go:ResetStalled
WITH stalled AS (
	SELECT {id} FROM %s
	WHERE
		{state} = 'processing' AND
		NOW() - {started_at} > (%s * '1 second'::interval) AND
		{num_resets} < %s
	FOR UPDATE SKIP LOCKED
)
UPDATE %s
SET
	{state} = 'queued',
	{started_at} = null,
	{num_resets} = {num_resets} + 1
WHERE {id} IN (SELECT {id} FROM stalled)
RETURNING {id}
`

const resetStalledMaxResetsQuery = `
-- source: internal/workerutil/store.go:ResetStalled
WITH stalled AS (
	SELECT {id} FROM %s
	WHERE
		{state} = 'processing' AND
		NOW() - {started_at} > (%s * '1 second'::interval) AND
		{num_resets} >= %s
	FOR UPDATE SKIP LOCKED
)
UPDATE %s
SET
	{state} = 'errored',
	{finished_at} = clock_timestamp(),
	{failure_message} = 'failed to process'
WHERE {id} IN (SELECT {id} FROM stalled)
RETURNING {id}
`

func (s *store) formatQuery(query string, args ...interface{}) *sqlf.Query {
	return sqlf.Sprintf(s.columnReplacer.Replace(query), args...)
}

// quote wraps the given string in a *sqlf.Query so that it is not passed to the database
// as a parameter. It is necessary to quote things such as table names, columns, and other
// expressions that are not simple values.
func quote(s string) *sqlf.Query {
	return sqlf.Sprintf(s)
}

// makeConditionSuffix returns a *sqlf.Query containing "AND {c1 AND c2 AND ...}" when the
// given set of conditions is non-empty, and an empty string otherwise.
func makeConditionSuffix(conditions []*sqlf.Query) *sqlf.Query {
	if len(conditions) == 0 {
		return sqlf.Sprintf("")
	}

	var quotedConditions []*sqlf.Query
	for _, condition := range conditions {
		// Ensure everything is quoted in case the condition has an OR
		quotedConditions = append(quotedConditions, sqlf.Sprintf("(%s)", condition))
	}

	return sqlf.Sprintf("AND %s", sqlf.Join(quotedConditions, " AND "))
}
