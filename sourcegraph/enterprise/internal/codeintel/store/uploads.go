package store

import (
	"context"
	"database/sql"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/pkg/errors"
)

// Upload is a subset of the lsif_uploads table and stores both processed and unprocessed
// records.
type Upload struct {
	ID             int        `json:"id"`
	Commit         string     `json:"commit"`
	Root           string     `json:"root"`
	VisibleAtTip   bool       `json:"visibleAtTip"`
	UploadedAt     time.Time  `json:"uploadedAt"`
	State          string     `json:"state"`
	FailureMessage *string    `json:"failureMessage"`
	StartedAt      *time.Time `json:"startedAt"`
	FinishedAt     *time.Time `json:"finishedAt"`
	ProcessAfter   *time.Time `json:"processAfter"`
	NumResets      int        `json:"numResets"`
	RepositoryID   int        `json:"repositoryId"`
	Indexer        string     `json:"indexer"`
	NumParts       int        `json:"numParts"`
	UploadedParts  []int      `json:"uploadedParts"`
	Rank           *int       `json:"placeInQueue"`
}

// scanUploads scans a slice of uploads from the return value of `*store.query`.
func scanUploads(rows *sql.Rows, queryErr error) (_ []Upload, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	var uploads []Upload
	for rows.Next() {
		var upload Upload
		var rawUploadedParts []sql.NullInt32
		if err := rows.Scan(
			&upload.ID,
			&upload.Commit,
			&upload.Root,
			&upload.VisibleAtTip,
			&upload.UploadedAt,
			&upload.State,
			&upload.FailureMessage,
			&upload.StartedAt,
			&upload.FinishedAt,
			&upload.ProcessAfter,
			&upload.NumResets,
			&upload.RepositoryID,
			&upload.Indexer,
			&upload.NumParts,
			pq.Array(&rawUploadedParts),
			&upload.Rank,
		); err != nil {
			return nil, err
		}

		var uploadedParts = []int{}
		for _, uploadedPart := range rawUploadedParts {
			uploadedParts = append(uploadedParts, int(uploadedPart.Int32))
		}
		upload.UploadedParts = uploadedParts

		uploads = append(uploads, upload)
	}

	return uploads, nil
}

// scanFirstUpload scans a slice of uploads from the return value of `*store.query` and returns the first.
func scanFirstUpload(rows *sql.Rows, err error) (Upload, bool, error) {
	uploads, err := scanUploads(rows, err)
	if err != nil || len(uploads) == 0 {
		return Upload{}, false, err
	}
	return uploads[0], true, nil
}

// scanFirstUploadInterface scans a slice of uploads from the return value of `*store.query` and returns the first.
func scanFirstUploadInterface(rows *sql.Rows, err error) (interface{}, bool, error) {
	return scanFirstUpload(rows, err)
}

// scanStates scans pairs of id/states from the return value of `*store.query`.
func scanStates(rows *sql.Rows, queryErr error) (_ map[int]string, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	states := map[int]string{}
	for rows.Next() {
		var id int
		var state string
		if err := rows.Scan(&id, &state); err != nil {
			return nil, err
		}

		states[id] = state
	}

	return states, nil
}

// scanVisibility scans pairs of id/visibleAtTip from the return value of `*store.query`.
func scanVisibilities(rows *sql.Rows, queryErr error) (_ map[int]bool, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	visibilities := map[int]bool{}
	for rows.Next() {
		var id int
		var visibleAtTip bool
		if err := rows.Scan(&id, &visibleAtTip); err != nil {
			return nil, err
		}

		visibilities[id] = visibleAtTip
	}

	return visibilities, nil
}

// GetUploadByID returns an upload by its identifier and boolean flag indicating its existence.
func (s *store) GetUploadByID(ctx context.Context, id int) (Upload, bool, error) {
	return scanFirstUpload(s.query(ctx, sqlf.Sprintf(`
		SELECT
			u.id,
			u.commit,
			u.root,
			u.visible_at_tip,
			u.uploaded_at,
			u.state,
			u.failure_message,
			u.started_at,
			u.finished_at,
			u.process_after,
			u.num_resets,
			u.repository_id,
			u.indexer,
			u.num_parts,
			u.uploaded_parts,
			s.rank
		FROM lsif_uploads u
		LEFT JOIN (
			SELECT r.id, RANK() OVER (ORDER BY COALESCE(r.process_after, r.uploaded_at)) as rank
			FROM lsif_uploads r
			WHERE r.state = 'queued'
		) s
		ON u.id = s.id
		WHERE u.id = %s
	`, id)))
}

type GetUploadsOptions struct {
	RepositoryID int
	State        string
	Term         string
	VisibleAtTip bool
	Limit        int
	Offset       int
}

// GetUploads returns a list of uploads and the total count of records matching the given conditions.
func (s *store) GetUploads(ctx context.Context, opts GetUploadsOptions) (_ []Upload, _ int, err error) {
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
		conds = append(conds, makeSearchCondition(opts.Term))
	}
	if opts.State != "" {
		conds = append(conds, sqlf.Sprintf("u.state = %s", opts.State))
	}
	if opts.VisibleAtTip {
		conds = append(conds, sqlf.Sprintf("u.visible_at_tip = true"))
	}

	if len(conds) == 0 {
		conds = append(conds, sqlf.Sprintf("TRUE"))
	}

	count, _, err := scanFirstInt(tx.query(
		ctx,
		sqlf.Sprintf(`SELECT COUNT(*) FROM lsif_uploads u WHERE %s`, sqlf.Join(conds, " AND ")),
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
				u.failure_message,
				u.started_at,
				u.finished_at,
				u.process_after,
				u.num_resets,
				u.repository_id,
				u.indexer,
				u.num_parts,
				u.uploaded_parts,
				s.rank
			FROM lsif_uploads u
			LEFT JOIN (
				SELECT r.id, RANK() OVER (ORDER BY COALESCE(r.process_after, r.uploaded_at)) as rank
				FROM lsif_uploads r
				WHERE r.state = 'queued'
			) s
			ON u.id = s.id
			WHERE %s ORDER BY uploaded_at DESC LIMIT %d OFFSET %d
		`, sqlf.Join(conds, " AND "), opts.Limit, opts.Offset),
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
		"failure_message",
	}

	var termConds []*sqlf.Query
	for _, column := range searchableColumns {
		termConds = append(termConds, sqlf.Sprintf("u."+column+" LIKE %s", "%"+term+"%"))
	}

	return sqlf.Sprintf("(%s)", sqlf.Join(termConds, " OR "))
}

// QueueSize returns the number of uploads in the queued state.
func (s *store) QueueSize(ctx context.Context) (int, error) {
	count, _, err := scanFirstInt(s.query(ctx, sqlf.Sprintf(`SELECT COUNT(*) FROM lsif_uploads WHERE state = 'queued'`)))
	return count, err
}

// InsertUpload inserts a new upload and returns its identifier.
func (s *store) InsertUpload(ctx context.Context, upload Upload) (int, error) {
	if upload.UploadedParts == nil {
		upload.UploadedParts = []int{}
	}

	id, _, err := scanFirstInt(s.query(
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
func (s *store) AddUploadPart(ctx context.Context, uploadID, partIndex int) error {
	return s.queryForEffect(ctx, sqlf.Sprintf(`
		UPDATE lsif_uploads
		SET uploaded_parts = array(SELECT DISTINCT * FROM unnest(array_append(uploaded_parts, %s)))
		WHERE id = %s
	`, partIndex, uploadID))
}

// MarkQueued updates the state of the upload to queued.
func (s *store) MarkQueued(ctx context.Context, id int) error {
	return s.queryForEffect(ctx, sqlf.Sprintf(`UPDATE lsif_uploads SET state = 'queued' WHERE id = %s`, id))
}

// MarkComplete updates the state of the upload to complete.
func (s *store) MarkComplete(ctx context.Context, id int) (err error) {
	return s.queryForEffect(ctx, sqlf.Sprintf(`
		UPDATE lsif_uploads
		SET state = 'completed', finished_at = clock_timestamp()
		WHERE id = %s
	`, id))
}

// MarkErrored updates the state of the upload to errored and updates the failure summary data.
func (s *store) MarkErrored(ctx context.Context, id int, failureMessage string) (err error) {
	return s.queryForEffect(ctx, sqlf.Sprintf(`
		UPDATE lsif_uploads
		SET state = 'errored', finished_at = clock_timestamp(), failure_message = %s
		WHERE id = %s
	`, failureMessage, id))
}

var uploadColumnsWithNullRank = []*sqlf.Query{
	sqlf.Sprintf("id"),
	sqlf.Sprintf("commit"),
	sqlf.Sprintf("root"),
	sqlf.Sprintf("visible_at_tip"),
	sqlf.Sprintf("uploaded_at"),
	sqlf.Sprintf("state"),
	sqlf.Sprintf("failure_message"),
	sqlf.Sprintf("started_at"),
	sqlf.Sprintf("finished_at"),
	sqlf.Sprintf("process_after"),
	sqlf.Sprintf("num_resets"),
	sqlf.Sprintf("repository_id"),
	sqlf.Sprintf("indexer"),
	sqlf.Sprintf("num_parts"),
	sqlf.Sprintf("uploaded_parts"),
	sqlf.Sprintf("NULL"),
}

// Dequeue selects the oldest queued upload and locks it with a transaction. If there is such an upload, the
// upload is returned along with a store instance which wraps the transaction. This transaction must be closed.
// If there is no such unlocked upload, a zero-value upload and nil store will be returned along with a false
// valued flag. This method must not be called from within a transaction.
func (s *store) Dequeue(ctx context.Context) (Upload, Store, bool, error) {
	upload, tx, ok, err := s.dequeueRecord(ctx, "lsif_uploads", uploadColumnsWithNullRank, sqlf.Sprintf("uploaded_at"), scanFirstUploadInterface)
	if err != nil || !ok {
		return Upload{}, tx, ok, err
	}

	return upload.(Upload), tx, true, nil
}

// Requeue updates the state of the upload to queued and adds a processing delay before the next dequeue attempt.
func (s *store) Requeue(ctx context.Context, id int, after time.Time) error {
	return s.queryForEffect(ctx, sqlf.Sprintf(`UPDATE lsif_uploads SET state = 'queued', process_after = %s WHERE id = %s`, after, id))
}

// GetStates returns the states for the uploads with the given identifiers.
func (s *store) GetStates(ctx context.Context, ids []int) (map[int]string, error) {
	return scanStates(s.query(ctx, sqlf.Sprintf(`
		SELECT id, state FROM lsif_uploads
		WHERE id IN (%s)
	`, sqlf.Join(intsToQueries(ids), ", "))))
}

// DeleteUploadByID deletes an upload by its identifier. If the upload was visible at the tip of its repository's default branch,
// the visibility of all uploads for that repository are recalculated. The getTipCommit function is expected to return the newest
// commit on the default branch when invoked.
func (s *store) DeleteUploadByID(ctx context.Context, id int, getTipCommit GetTipCommitFn) (_ bool, err error) {
	tx, started, err := s.transact(ctx)
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
			tipCommit, err := getTipCommit(ctx, repositoryID)
			if err != nil {
				return false, err
			}

			if err := tx.UpdateDumpsVisibleFromTip(ctx, repositoryID, tipCommit); err != nil {
				return false, errors.Wrap(err, "s.UpdateDumpsVisibleFromTip")
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

// UploadMaxNumResets is the maximum number of times an upload can be reset. If an upload's
// failed attempts counter reaches this threshold, it will be moved into "errored" rather than
// "queued" on its next reset.
const UploadMaxNumResets = 3

// ResetStalled moves all unlocked uploads processing for more than `StalledUploadMaxAge` back to the queued state.
// In order to prevent input that continually crashes worker instances, uploads that have been reset more than
// UploadMaxNumResets times will be marked as errored. This method returns a list of updated and errored upload
// identifiers.
func (s *store) ResetStalled(ctx context.Context, now time.Time) ([]int, []int, error) {
	resetIDs, err := scanInts(s.query(
		ctx,
		sqlf.Sprintf(`
			UPDATE lsif_uploads u
			SET state = 'queued', started_at = null, num_resets = num_resets + 1
			WHERE id = ANY(
				SELECT id FROM lsif_uploads
				WHERE
					state = 'processing' AND
					%s - started_at > (%s * interval '1 second') AND
					num_resets < %s
				FOR UPDATE SKIP LOCKED
			)
			RETURNING u.id
		`, now.UTC(), StalledUploadMaxAge/time.Second, UploadMaxNumResets),
	))
	if err != nil {
		return nil, nil, err
	}

	erroredIDs, err := scanInts(s.query(
		ctx,
		sqlf.Sprintf(`
			UPDATE lsif_uploads u
			SET state = 'errored', finished_at = clock_timestamp(), failure_message = 'failed to process'
			WHERE id = ANY(
				SELECT id FROM lsif_uploads
				WHERE
					state = 'processing' AND
					%s - started_at > (%s * interval '1 second') AND
					num_resets >= %s
				FOR UPDATE SKIP LOCKED
			)
			RETURNING u.id
		`, now.UTC(), StalledUploadMaxAge/time.Second, UploadMaxNumResets),
	))
	if err != nil {
		return nil, nil, err
	}

	return resetIDs, erroredIDs, nil
}
