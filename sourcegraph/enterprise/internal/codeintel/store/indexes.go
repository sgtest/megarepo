package store

import (
	"context"
	"database/sql"
	"time"

	"github.com/keegancsmith/sqlf"
)

// Index is a subset of the lsif_indexes table and stores both processed and unprocessed
// records.
type Index struct {
	ID             int        `json:"id"`
	Commit         string     `json:"commit"`
	QueuedAt       time.Time  `json:"queuedAt"`
	State          string     `json:"state"`
	FailureMessage *string    `json:"failureMessage"`
	StartedAt      *time.Time `json:"startedAt"`
	FinishedAt     *time.Time `json:"finishedAt"`
	ProcessAfter   *time.Time `json:"processAfter"`
	NumResets      int        `json:"numResets"`
	RepositoryID   int        `json:"repositoryId"`
	Rank           *int       `json:"placeInQueue"`
}

// scanIndexes scans a slice of indexes from the return value of `*store.query`.
func scanIndexes(rows *sql.Rows, queryErr error) (_ []Index, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	var indexes []Index
	for rows.Next() {
		var index Index
		if err := rows.Scan(
			&index.ID,
			&index.Commit,
			&index.QueuedAt,
			&index.State,
			&index.FailureMessage,
			&index.StartedAt,
			&index.FinishedAt,
			&index.ProcessAfter,
			&index.NumResets,
			&index.RepositoryID,
			&index.Rank,
		); err != nil {
			return nil, err
		}

		indexes = append(indexes, index)
	}

	return indexes, nil
}

// scanFirstIndex scans a slice of indexes from the return value of `*store.query` and returns the first.
func scanFirstIndex(rows *sql.Rows, err error) (Index, bool, error) {
	indexes, err := scanIndexes(rows, err)
	if err != nil || len(indexes) == 0 {
		return Index{}, false, err
	}
	return indexes[0], true, nil
}

// scanFirstIndexInterface scans a slice of indexes from the return value of `*store.query` and returns the first.
func scanFirstIndexInterface(rows *sql.Rows, err error) (interface{}, bool, error) {
	return scanFirstIndex(rows, err)
}

// GetIndexByID returns an index by its identifier and boolean flag indicating its existence.
func (s *store) GetIndexByID(ctx context.Context, id int) (Index, bool, error) {
	return scanFirstIndex(s.query(ctx, sqlf.Sprintf(`
		SELECT
			u.id,
			u.commit,
			u.queued_at,
			u.state,
			u.failure_message,
			u.started_at,
			u.finished_at,
			u.process_after,
			u.num_resets,
			u.repository_id,
			s.rank
		FROM lsif_indexes u
		LEFT JOIN (
			SELECT r.id, RANK() OVER (ORDER BY COALESCE(r.process_after, r.queued_at)) as rank
			FROM lsif_indexes r
			WHERE r.state = 'queued'
		) s
		ON u.id = s.id
		WHERE u.id = %s
	`, id)))
}

type GetIndexesOptions struct {
	RepositoryID int
	State        string
	Term         string
	Limit        int
	Offset       int
}

// GetIndexes returns a list of indexes and the total count of records matching the given conditions.
func (s *store) GetIndexes(ctx context.Context, opts GetIndexesOptions) (_ []Index, _ int, err error) {
	tx, started, err := s.transact(ctx)
	if err != nil {
		return nil, 0, err
	}
	if started {
		defer func() { err = tx.Done(err) }()
	}

	var conds []*sqlf.Query

	if opts.RepositoryID != 0 {
		conds = append(conds, sqlf.Sprintf("u.repository_id = %s", opts.RepositoryID))
	}
	if opts.Term != "" {
		conds = append(conds, makeIndexSearchCondition(opts.Term))
	}
	if opts.State != "" {
		conds = append(conds, sqlf.Sprintf("u.state = %s", opts.State))
	}

	if len(conds) == 0 {
		conds = append(conds, sqlf.Sprintf("TRUE"))
	}

	count, _, err := scanFirstInt(tx.query(
		ctx,
		sqlf.Sprintf(`SELECT COUNT(*) FROM lsif_indexes u WHERE %s`, sqlf.Join(conds, " AND ")),
	))
	if err != nil {
		return nil, 0, err
	}

	indexes, err := scanIndexes(tx.query(
		ctx,
		sqlf.Sprintf(`
			SELECT
				u.id,
				u.commit,
				u.queued_at,
				u.state,
				u.failure_message,
				u.started_at,
				u.finished_at,
				u.process_after,
				u.num_resets,
				u.repository_id,
				s.rank
			FROM lsif_indexes u
			LEFT JOIN (
				SELECT r.id, RANK() OVER (ORDER BY COALESCE(r.process_after, r.queued_at)) as rank
				FROM lsif_indexes r
				WHERE r.state = 'queued'
			) s
			ON u.id = s.id
			WHERE %s ORDER BY queued_at DESC LIMIT %d OFFSET %d
		`, sqlf.Join(conds, " AND "), opts.Limit, opts.Offset),
	))
	if err != nil {
		return nil, 0, err
	}

	return indexes, count, nil
}

// makeIndexSearchCondition returns a disjunction of LIKE clauses against all searchable columns of an index.
func makeIndexSearchCondition(term string) *sqlf.Query {
	searchableColumns := []string{
		"commit",
		"failure_message",
	}

	var termConds []*sqlf.Query
	for _, column := range searchableColumns {
		termConds = append(termConds, sqlf.Sprintf("u."+column+" LIKE %s", "%"+term+"%"))
	}

	return sqlf.Sprintf("(%s)", sqlf.Join(termConds, " OR "))
}

// IndexQueueSize returns the number of indexes in the queued state.
func (s *store) IndexQueueSize(ctx context.Context) (int, error) {
	count, _, err := scanFirstInt(s.query(
		ctx,
		sqlf.Sprintf(`SELECT COUNT(*) FROM lsif_indexes WHERE state = 'queued'`),
	))

	return count, err
}

// IsQueued returns true if there is an index or an upload for the repository and commit.
func (s *store) IsQueued(ctx context.Context, repositoryID int, commit string) (bool, error) {
	count, _, err := scanFirstInt(s.query(ctx, sqlf.Sprintf(`
		SELECT COUNT(*) WHERE EXISTS (
			SELECT id FROM lsif_uploads WHERE repository_id = %s AND commit = %s
			UNION
			SELECT id FROM lsif_indexes WHERE repository_id = %s AND commit = %s
		)
	`, repositoryID, commit, repositoryID, commit)))

	return count > 0, err
}

// InsertIndex inserts a new index and returns its identifier.
func (s *store) InsertIndex(ctx context.Context, index Index) (int, error) {
	id, _, err := scanFirstInt(s.query(
		ctx,
		sqlf.Sprintf(`
			INSERT INTO lsif_indexes (
				commit,
				repository_id,
				state
			) VALUES (%s, %s, %s)
			RETURNING id
		`, index.Commit, index.RepositoryID, index.State),
	))

	return id, err
}

// MarkIndexComplete updates the state of the index to complete.
func (s *store) MarkIndexComplete(ctx context.Context, id int) (err error) {
	return s.queryForEffect(ctx, sqlf.Sprintf(`
		UPDATE lsif_indexes
		SET state = 'completed', finished_at = clock_timestamp()
		WHERE id = %s
	`, id))
}

// MarkIndexErrored updates the state of the index to errored and updates the failure summary data.
func (s *store) MarkIndexErrored(ctx context.Context, id int, failureMessage string) (err error) {
	return s.queryForEffect(ctx, sqlf.Sprintf(`
		UPDATE lsif_indexes
		SET state = 'errored', finished_at = clock_timestamp(), failure_message = %s
		WHERE id = %s
	`, failureMessage, id))
}

var indexColumnsWithNullRank = []*sqlf.Query{
	sqlf.Sprintf("id"),
	sqlf.Sprintf("commit"),
	sqlf.Sprintf("queued_at"),
	sqlf.Sprintf("state"),
	sqlf.Sprintf("failure_message"),
	sqlf.Sprintf("started_at"),
	sqlf.Sprintf("finished_at"),
	sqlf.Sprintf("process_after"),
	sqlf.Sprintf("num_resets"),
	sqlf.Sprintf("repository_id"),
	sqlf.Sprintf("NULL"),
}

// DequeueIndex selects the oldest queued index and locks it with a transaction. If there is such an index,
// the index is returned along with a store instance which wraps the transaction. This transaction must be
// closed. If there is no such unlocked index, a zero-value index and nil store will be returned along with
// a false valued flag. This method must not be called from within a transaction.
func (s *store) DequeueIndex(ctx context.Context) (Index, Store, bool, error) {
	index, tx, ok, err := s.dequeueRecord(ctx, "lsif_indexes", indexColumnsWithNullRank, sqlf.Sprintf("queued_at"), scanFirstIndexInterface)
	if err != nil || !ok {
		return Index{}, tx, ok, err
	}

	return index.(Index), tx, true, nil
}

// RequeueIndex updates the state of the index to queued and adds a processing delay before the next dequeue attempt.
func (s *store) RequeueIndex(ctx context.Context, id int, after time.Time) error {
	return s.queryForEffect(ctx, sqlf.Sprintf(`UPDATE lsif_indexes SET state = 'queued', process_after = %s WHERE id = %s`, after, id))
}

// DeleteIndexByID deletes an index by its identifier.
func (s *store) DeleteIndexByID(ctx context.Context, id int) (_ bool, err error) {
	tx, started, err := s.transact(ctx)
	if err != nil {
		return false, err
	}
	if started {
		defer func() { err = tx.Done(err) }()
	}

	_, exists, err := scanFirstInt(tx.query(
		ctx,
		sqlf.Sprintf(`
			DELETE FROM lsif_indexes
			WHERE id = %s
			RETURNING repository_id
		`, id),
	))
	return exists, err
}

// StalledIndexMaxAge is the maximum allowable duration between updating the state of an
// index as "processing" and locking the index row during processing. An unlocked row that
// is marked as processing likely indicates that the indexer that dequeued the index has
// died. There should be a nearly-zero delay between these states during normal operation.
const StalledIndexMaxAge = time.Second * 5

// IndexMaxNumResets is the maximum number of times an index can be reset. If an index's
// failed attempts counter reaches this threshold, it will be moved into "errored" rather than
// "queued" on its next reset.
const IndexMaxNumResets = 3

// ResetStalledIndexes moves all unlocked index processing for more than `StalledIndexMaxAge` back to the
// queued state. In order to prevent input that continually crashes indexer instances, indexes that have
// been reset more than IndexMaxNumResets times will be marked as errored. This method returns a list of
// updated and errored index identifiers.
func (s *store) ResetStalledIndexes(ctx context.Context, now time.Time) ([]int, []int, error) {
	resetIDs, err := scanInts(s.query(
		ctx,
		sqlf.Sprintf(`
			UPDATE lsif_indexes u
			SET state = 'queued', started_at = null, num_resets = num_resets + 1
			WHERE id = ANY(
				SELECT id FROM lsif_indexes
				WHERE
					state = 'processing' AND
					%s - started_at > (%s * interval '1 second') AND
					num_resets < %s
				FOR UPDATE SKIP LOCKED
			)
			RETURNING u.id
		`, now.UTC(), StalledIndexMaxAge/time.Second, IndexMaxNumResets),
	))
	if err != nil {
		return nil, nil, err
	}

	erroredIDs, err := scanInts(s.query(
		ctx,
		sqlf.Sprintf(`
			UPDATE lsif_indexes u
			SET state = 'errored', finished_at = clock_timestamp(), failure_message = 'failed to process'
			WHERE id = ANY(
				SELECT id FROM lsif_indexes
				WHERE
					state = 'processing' AND
					%s - started_at > (%s * interval '1 second') AND
					num_resets >= %s
				FOR UPDATE SKIP LOCKED
			)
			RETURNING u.id
		`, now.UTC(), StalledIndexMaxAge/time.Second, IndexMaxNumResets),
	))
	if err != nil {
		return nil, nil, err
	}

	return resetIDs, erroredIDs, nil
}
