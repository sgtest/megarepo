package db

import (
	"context"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/pkg/errors"
)

// Upload is a subset of the lsif_uploads table and stores both processed and unprocessed
// records.
type Upload struct {
	ID                int        `json:"id"`
	Commit            string     `json:"commit"`
	Root              string     `json:"root"`
	VisibleAtTip      bool       `json:"visibleAtTip"`
	UploadedAt        time.Time  `json:"uploadedAt"`
	State             string     `json:"state"`
	FailureSummary    *string    `json:"failureSummary"`
	FailureStacktrace *string    `json:"failureStacktrace"`
	StartedAt         *time.Time `json:"startedAt"`
	FinishedAt        *time.Time `json:"finishedAt"`
	RepositoryID      int        `json:"repositoryId"`
	Indexer           string     `json:"indexer"`
	NumParts          int        `json:"numParts"`
	UploadedParts     []int      `json:"uploadedParts"`
	Rank              *int       `json:"placeInQueue"`
}

// GetUploadByID returns an upload by its identifier and boolean flag indicating its existence.
func (db *dbImpl) GetUploadByID(ctx context.Context, id int) (Upload, bool, error) {
	return scanFirstUpload(db.query(ctx, sqlf.Sprintf(`
		SELECT
			u.id,
			u.commit,
			u.root,
			u.visible_at_tip,
			u.uploaded_at,
			u.state,
			u.failure_summary,
			u.failure_stacktrace,
			u.started_at,
			u.finished_at,
			u.repository_id,
			u.indexer,
			u.num_parts,
			u.uploaded_parts,
			s.rank
		FROM lsif_uploads u
		LEFT JOIN (
			SELECT r.id, RANK() OVER (ORDER BY r.uploaded_at) as rank
			FROM lsif_uploads r
			WHERE r.state = 'queued'
		) s
		ON u.id = s.id
		WHERE u.id = %s
	`, id)))
}

// GetUploadsByRepo returns a list of uploads for a particular repo and the total count of records matching the given conditions.
func (db *dbImpl) GetUploadsByRepo(ctx context.Context, repositoryID int, state, term string, visibleAtTip bool, limit, offset int) (_ []Upload, _ int, err error) {
	tx, started, err := db.transact(ctx)
	if err != nil {
		return nil, 0, err
	}
	if started {
		defer func() { err = tx.Done(err) }()
	}

	conds := []*sqlf.Query{
		sqlf.Sprintf("u.repository_id = %s", repositoryID),
	}
	if term != "" {
		conds = append(conds, makeSearchCondition(term))
	}
	if state != "" {
		conds = append(conds, sqlf.Sprintf("u.state = %s", state))
	}
	if visibleAtTip {
		conds = append(conds, sqlf.Sprintf("u.visible_at_tip = true"))
	}

	count, _, err := scanFirstInt(tx.query(
		ctx,
		sqlf.Sprintf(`SELECT COUNT(1) FROM lsif_uploads u WHERE %s`, sqlf.Join(conds, " AND ")),
	))
	if err != nil {
		return nil, 0, err
	}

	uploads, err := scanUploads(tx.query(
		ctx,
		sqlf.Sprintf(`
			SELECT
				u.id,
				u.commit,
				u.root,
				u.visible_at_tip,
				u.uploaded_at,
				u.state,
				u.failure_summary,
				u.failure_stacktrace,
				u.started_at,
				u.finished_at,
				u.repository_id,
				u.indexer,
				u.num_parts,
				u.uploaded_parts,
				s.rank
			FROM lsif_uploads u
			LEFT JOIN (
				SELECT r.id, RANK() OVER (ORDER BY r.uploaded_at) as rank
				FROM lsif_uploads r
				WHERE r.state = 'queued'
			) s
			ON u.id = s.id
			WHERE %s ORDER BY uploaded_at DESC LIMIT %d OFFSET %d
		`, sqlf.Join(conds, " AND "), limit, offset),
	))
	if err != nil {
		return nil, 0, err
	}

	return uploads, count, nil
}

// makeSearchCondition returns a disjunction of LIKE clauses against all searchable columns of an upload.
func makeSearchCondition(term string) *sqlf.Query {
	searchableColumns := []string{
		"commit",
		"root",
		"indexer",
		"failure_summary",
		"failure_stacktrace",
	}

	var termConds []*sqlf.Query
	for _, column := range searchableColumns {
		termConds = append(termConds, sqlf.Sprintf("u."+column+" LIKE %s", "%"+term+"%"))
	}

	return sqlf.Sprintf("(%s)", sqlf.Join(termConds, " OR "))
}

// QueueSize returns the number of uploads in the queued state.
func (db *dbImpl) QueueSize(ctx context.Context) (int, error) {
	count, _, err := scanFirstInt(db.query(ctx, sqlf.Sprintf(`SELECT COUNT(*) FROM lsif_uploads WHERE state = 'queued'`)))
	return count, err
}

// InsertUpload inserts a new upload and returns its identifier.
func (db *dbImpl) InsertUpload(ctx context.Context, upload *Upload) (int, error) {
	if upload.UploadedParts == nil {
		upload.UploadedParts = []int{}
	}

	id, _, err := scanFirstInt(db.query(
		ctx,
		sqlf.Sprintf(`
			INSERT INTO lsif_uploads (
				commit,
				root,
				repository_id,
				indexer,
				state,
				num_parts,
				uploaded_parts
			) VALUES (%s, %s, %s, %s, %s, %s, %s)
			RETURNING id
		`,
			upload.Commit,
			upload.Root,
			upload.RepositoryID,
			upload.Indexer,
			upload.State,
			upload.NumParts,
			pq.Array(upload.UploadedParts),
		),
	))

	return id, err
}

// AddUploadPart adds the part index to the given upload's uploaded parts array. This method is idempotent
// (the resulting array is deduplicated on update).
func (db *dbImpl) AddUploadPart(ctx context.Context, uploadID, partIndex int) error {
	return db.exec(ctx, sqlf.Sprintf(`
		UPDATE lsif_uploads
		SET uploaded_parts = array(SELECT DISTINCT * FROM unnest(array_append(uploaded_parts, %s)))
		WHERE id = %s
	`, partIndex, uploadID))
}

// MarkQueued updates the state of the upload to queued.
func (db *dbImpl) MarkQueued(ctx context.Context, uploadID int) error {
	return db.exec(ctx, sqlf.Sprintf(`UPDATE lsif_uploads SET state = 'queued' WHERE id = %s`, uploadID))
}

// MarkComplete updates the state of the upload to complete.
func (db *dbImpl) MarkComplete(ctx context.Context, id int) (err error) {
	return db.exec(ctx, sqlf.Sprintf(`
		UPDATE lsif_uploads
		SET state = 'completed', finished_at = now()
		WHERE id = %s
	`, id))
}

// MarkErrored updates the state of the upload to errored and updates the failure summary data.
func (db *dbImpl) MarkErrored(ctx context.Context, id int, failureSummary, failureStacktrace string) (err error) {
	return db.exec(ctx, sqlf.Sprintf(`
		UPDATE lsif_uploads
		SET state = 'errored', finished_at = now(), failure_summary = %s, failure_stacktrace = %s
		WHERE id = %s
	`, failureSummary, failureStacktrace, id))
}

// ErrDequeueTransaction occurs when Dequeue is called from inside a transaction.
var ErrDequeueTransaction = errors.New("unexpected transaction")

// Dequeue selects the oldest queued upload and locks it with a transaction. If there is such an upload, the
// upload is returned along with a JobHandle instance which wraps the transaction. This handle must be closed.
// If there is no such unlocked upload, a zero-value upload and nil-job handle will be returned along with a
// false-valued flag. This method must not be called from within a transaction.
func (db *dbImpl) Dequeue(ctx context.Context) (Upload, JobHandle, bool, error) {
	for {
		// First, we try to select an eligible upload record outside of a transaction. This will skip
		// any rows that are currently locked inside of a transaction of another worker process.
		id, ok, err := scanFirstInt(db.query(ctx, sqlf.Sprintf(`
			UPDATE lsif_uploads u SET state = 'processing', started_at = now() WHERE id = (
				SELECT id FROM lsif_uploads
				WHERE state = 'queued'
				ORDER BY uploaded_at
				FOR UPDATE SKIP LOCKED LIMIT 1
			)
			RETURNING u.id
		`)))
		if err != nil || !ok {
			return Upload{}, nil, false, err
		}

		upload, jobHandle, ok, err := db.dequeue(ctx, id)
		if err != nil {
			// This will occur if we selected an ID that raced with another worker. If both workers
			// select the same ID and the other process begins its transaction first, this condition
			// will occur. We'll re-try the process by selecting a fresh ID.
			if err == ErrDequeueRace {
				continue
			}

			return Upload{}, nil, false, errors.Wrap(err, "db.dequeue")
		}

		return upload, jobHandle, ok, nil
	}
}

// ErrDequeueRace occurs when an upload selected for dequeue has been locked by another worker.
var ErrDequeueRace = errors.New("unexpected transaction")

// dequeue begins a transaction to lock an upload record for updating. This marks the upload as
// ineligible for a dequeue to other worker processes. All updates to the database while this record
// is being processes should happen through the JobHandle's transaction, which must be explicitly
// closed (via CloseTx) at the end of processing by the caller.
func (db *dbImpl) dequeue(ctx context.Context, id int) (_ Upload, _ JobHandle, _ bool, err error) {
	tx, started, err := db.transact(ctx)
	if err != nil {
		return Upload{}, nil, false, err
	}
	if !started {
		return Upload{}, nil, false, ErrDequeueTransaction
	}

	// SKIP LOCKED is necessary not to block on this select. We allow the database driver to return
	// sql.ErrNoRows on this condition so we can determine if we need to select a new upload to process
	// on race conditions with other worker processes.
	upload, exists, err := scanFirstUpload(tx.query(
		ctx,
		sqlf.Sprintf(`
			SELECT
				u.id,
				u.commit,
				u.root,
				u.visible_at_tip,
				u.uploaded_at,
				u.state,
				u.failure_summary,
				u.failure_stacktrace,
				u.started_at,
				u.finished_at,
				u.repository_id,
				u.indexer,
				u.num_parts,
				u.uploaded_parts,
				NULL
			FROM lsif_uploads u
			WHERE id = %s
			FOR UPDATE SKIP LOCKED
			LIMIT 1
		`, id),
	))
	if err != nil {
		return Upload{}, nil, false, tx.Done(err)
	}
	if !exists {
		return Upload{}, nil, false, tx.Done(ErrDequeueRace)
	}
	return upload, &jobHandleImpl{db: tx, id: id}, true, nil
}

// GetStates returns the states for the uploads with the given identifiers.
func (db *dbImpl) GetStates(ctx context.Context, ids []int) (map[int]string, error) {
	return scanStates(db.query(ctx, sqlf.Sprintf(`
		SELECT id, state FROM lsif_uploads
		WHERE id IN (%s)
	`, sqlf.Join(intsToQueries(ids), ", "))))
}

// DeleteUploadByID deletes an upload by its identifier. If the upload was visible at the tip of its repository's default branch,
// the visibility of all uploads for that repository are recalculated. The given function is expected to return the newest commit
// on the default branch when invoked.
func (db *dbImpl) DeleteUploadByID(ctx context.Context, id int, getTipCommit GetTipCommitFn) (_ bool, err error) {
	tx, started, err := db.transact(ctx)
	if err != nil {
		return false, err
	}
	if started {
		defer func() { err = tx.Done(err) }()
	}

	visibilities, err := scanVisibilities(tx.query(
		ctx,
		sqlf.Sprintf(`
			DELETE FROM lsif_uploads
			WHERE id = %s
			RETURNING repository_id, visible_at_tip
		`, id),
	))
	if err != nil {
		return false, err
	}

	for repositoryID, visibleAtTip := range visibilities {
		if visibleAtTip {
			tipCommit, err := getTipCommit(repositoryID)
			if err != nil {
				return false, err
			}

			if err := tx.UpdateDumpsVisibleFromTip(ctx, repositoryID, tipCommit); err != nil {
				return false, errors.Wrap(err, "db.UpdateDumpsVisibleFromTip")
			}
		}

		return true, nil
	}

	return false, nil
}

// StalledUploadMaxAge is the maximum allowable duration between updating the state of an
// upload as "processing" and locking the upload row during processing. An unlocked row that
// is marked as processing likely indicates that the worker that dequeued the upload has died.
// There should be a nearly-zero delay between these states during normal operation.
const StalledUploadMaxAge = time.Second * 5

// ResetStalled moves all unlocked uploads processing for more than `StalledUploadMaxAge` back to the queued state.
// This method returns a list of updated upload identifiers.
func (db *dbImpl) ResetStalled(ctx context.Context, now time.Time) ([]int, error) {
	ids, err := scanInts(db.query(
		ctx,
		sqlf.Sprintf(`
			UPDATE lsif_uploads u SET state = 'queued', started_at = null WHERE id = ANY(
				SELECT id FROM lsif_uploads
				WHERE state = 'processing' AND %s - started_at > (%s * interval '1 second')
				FOR UPDATE SKIP LOCKED
			)
			RETURNING u.id
		`, now.UTC(), StalledUploadMaxAge/time.Second),
	))
	if err != nil {
		return nil, err
	}

	return ids, nil
}
