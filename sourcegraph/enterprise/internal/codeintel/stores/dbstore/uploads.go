package dbstore

import (
	"context"
	"database/sql"
	"strconv"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
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
	FailureMessage    *string    `json:"failureMessage"`
	StartedAt         *time.Time `json:"startedAt"`
	FinishedAt        *time.Time `json:"finishedAt"`
	ProcessAfter      *time.Time `json:"processAfter"`
	NumResets         int        `json:"numResets"`
	NumFailures       int        `json:"numFailures"`
	RepositoryID      int        `json:"repositoryId"`
	RepositoryName    string     `json:"repositoryName"`
	Indexer           string     `json:"indexer"`
	NumParts          int        `json:"numParts"`
	UploadedParts     []int      `json:"uploadedParts"`
	UploadSize        *int64     `json:"uploadSize"`
	Rank              *int       `json:"placeInQueue"`
	AssociatedIndexID *int       `json:"associatedIndex"`
}

func (u Upload) RecordID() int {
	return u.ID
}

// scanUploads scans a slice of uploads from the return value of `*Store.query`.
func scanUploads(rows *sql.Rows, queryErr error) (_ []Upload, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

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
			&upload.NumFailures,
			&upload.RepositoryID,
			&upload.RepositoryName,
			&upload.Indexer,
			&upload.NumParts,
			pq.Array(&rawUploadedParts),
			&upload.UploadSize,
			&upload.AssociatedIndexID,
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

// scanFirstUpload scans a slice of uploads from the return value of `*Store.query` and returns the first.
func scanFirstUpload(rows *sql.Rows, err error) (Upload, bool, error) {
	uploads, err := scanUploads(rows, err)
	if err != nil || len(uploads) == 0 {
		return Upload{}, false, err
	}
	return uploads[0], true, nil
}

// scanFirstUploadRecord scans a slice of uploads from the return value of `*Store.query` and returns the first.
func scanFirstUploadRecord(rows *sql.Rows, err error) (workerutil.Record, bool, error) {
	return scanFirstUpload(rows, err)
}

// scanCounts scans pairs of id/counts from the return value of `*Store.query`.
func scanCounts(rows *sql.Rows, queryErr error) (_ map[int]int, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	visibilities := map[int]int{}
	for rows.Next() {
		var id int
		var count int
		if err := rows.Scan(&id, &count); err != nil {
			return nil, err
		}

		visibilities[id] = count
	}

	return visibilities, nil
}

// GetUploadByID returns an upload by its identifier and boolean flag indicating its existence.
func (s *Store) GetUploadByID(ctx context.Context, id int) (_ Upload, _ bool, err error) {
	ctx, endObservation := s.operations.getUploadByID.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("id", id),
	}})
	defer endObservation(1, observation.Args{})

	return scanFirstUpload(s.Store.Query(ctx, sqlf.Sprintf(getUploadByIDQuery, id)))
}

const uploadRankQueryFragment = `
SELECT
	r.id,
	ROW_NUMBER() OVER (ORDER BY COALESCE(r.process_after, r.uploaded_at), r.id) as rank
FROM lsif_uploads_with_repository_name r
WHERE r.state = 'queued'
`

const getUploadByIDQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:GetUploadByID
SELECT
	u.id,
	u.commit,
	u.root,
	EXISTS (SELECT 1 FROM lsif_uploads_visible_at_tip where repository_id = u.repository_id and upload_id = u.id) AS visible_at_tip,
	u.uploaded_at,
	u.state,
	u.failure_message,
	u.started_at,
	u.finished_at,
	u.process_after,
	u.num_resets,
	u.num_failures,
	u.repository_id,
	u.repository_name,
	u.indexer,
	u.num_parts,
	u.uploaded_parts,
	u.upload_size,
	u.associated_index_id,
	s.rank
FROM lsif_uploads_with_repository_name u
LEFT JOIN (` + uploadRankQueryFragment + `) s
ON u.id = s.id
WHERE u.state != 'deleted' AND u.id = %s
`

type GetUploadsOptions struct {
	RepositoryID   int
	State          string
	Term           string
	VisibleAtTip   bool
	UploadedBefore *time.Time
	OldestFirst    bool
	Limit          int
	Offset         int
}

// DeleteUploadsStuckUploading soft deletes any upload record that has been uploading since the given time.
func (s *Store) DeleteUploadsStuckUploading(ctx context.Context, uploadedBefore time.Time) (_ int, err error) {
	ctx, endObservation := s.operations.deleteUploadsStuckUploading.With(ctx, &err, observation.Args{LogFields: []log.Field{
		// TODO(efritz) - uploadedBefore should be a duration
	}})
	defer endObservation(1, observation.Args{})

	count, _, err := basestore.ScanFirstInt(s.Store.Query(ctx, sqlf.Sprintf(deleteUploadsStuckUploadingQuery, uploadedBefore)))
	return count, err
}

const deleteUploadsStuckUploadingQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:DeleteUploadsStuckUploading
WITH deleted AS (
	UPDATE lsif_uploads
	SET state = 'deleted'
	WHERE state = 'uploading' AND uploaded_at < %s
	RETURNING repository_id
)
SELECT count(*) FROM deleted
`

// GetUploads returns a list of uploads and the total count of records matching the given conditions.
func (s *Store) GetUploads(ctx context.Context, opts GetUploadsOptions) (_ []Upload, _ int, err error) {
	ctx, endObservation := s.operations.getUploads.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("repositoryID", opts.RepositoryID),
		log.String("state", opts.State),
		log.String("term", opts.Term),
		log.Bool("visibleAtTip", opts.VisibleAtTip),
		// TODO(efritz) - opts.UploadedBefore should be a duration
		log.Int("limit", opts.Limit),
		log.Int("offset", opts.Offset),
	}})
	defer endObservation(1, observation.Args{})

	tx, err := s.transact(ctx)
	if err != nil {
		return nil, 0, err
	}
	defer func() { err = tx.Done(err) }()

	var conds []*sqlf.Query
	if opts.RepositoryID != 0 {
		conds = append(conds, sqlf.Sprintf("u.repository_id = %s", opts.RepositoryID))
	}
	if opts.Term != "" {
		conds = append(conds, makeSearchCondition(opts.Term))
	}
	if opts.State != "" {
		conds = append(conds, sqlf.Sprintf("u.state = %s", opts.State))
	} else {
		conds = append(conds, sqlf.Sprintf("u.state != 'deleted'"))
	}
	if opts.VisibleAtTip {
		conds = append(conds, sqlf.Sprintf("EXISTS (SELECT 1 FROM lsif_uploads_visible_at_tip where repository_id = u.repository_id and upload_id = u.id)"))
	}
	if opts.UploadedBefore != nil {
		conds = append(conds, sqlf.Sprintf("u.uploaded_at < %s", *opts.UploadedBefore))
	}

	count, _, err := basestore.ScanFirstInt(tx.Store.Query(
		ctx,
		sqlf.Sprintf(getUploadsCountQuery, sqlf.Join(conds, " AND ")),
	))
	if err != nil {
		return nil, 0, err
	}

	var orderExpression *sqlf.Query
	if opts.OldestFirst {
		orderExpression = sqlf.Sprintf("uploaded_at")
	} else {
		orderExpression = sqlf.Sprintf("uploaded_at DESC")
	}

	uploads, err := scanUploads(tx.Store.Query(ctx, sqlf.Sprintf(getUploadsQuery, sqlf.Join(conds, " AND "), orderExpression, opts.Limit, opts.Offset)))
	if err != nil {
		return nil, 0, err
	}

	return uploads, count, nil
}

const getUploadsCountQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:GetUploads
SELECT COUNT(*) FROM lsif_uploads_with_repository_name u WHERE %s
`

const getUploadsQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:GetUploads
SELECT
	u.id,
	u.commit,
	u.root,
	EXISTS (SELECT 1 FROM lsif_uploads_visible_at_tip where repository_id = u.repository_id and upload_id = u.id) AS visible_at_tip,
	u.uploaded_at,
	u.state,
	u.failure_message,
	u.started_at,
	u.finished_at,
	u.process_after,
	u.num_resets,
	u.num_failures,
	u.repository_id,
	u.repository_name,
	u.indexer,
	u.num_parts,
	u.uploaded_parts,
	u.upload_size,
	u.associated_index_id,
	s.rank
FROM lsif_uploads_with_repository_name u
LEFT JOIN (` + uploadRankQueryFragment + `) s
ON u.id = s.id
WHERE %s ORDER BY %s LIMIT %d OFFSET %d
`

// makeSearchCondition returns a disjunction of LIKE clauses against all searchable columns of an upload.
func makeSearchCondition(term string) *sqlf.Query {
	searchableColumns := []string{
		"(u.state)::text",
		`u.repository_name`,
		"u.commit",
		"u.root",
		"u.indexer",
		"u.failure_message",
	}

	var termConds []*sqlf.Query
	for _, column := range searchableColumns {
		termConds = append(termConds, sqlf.Sprintf(column+" ILIKE %s", "%"+term+"%"))
	}

	return sqlf.Sprintf("(%s)", sqlf.Join(termConds, " OR "))
}

// InsertUpload inserts a new upload and returns its identifier.
func (s *Store) InsertUpload(ctx context.Context, upload Upload) (_ int, err error) {
	ctx, endObservation := s.operations.insertUpload.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("upload.ID", upload.ID),
	}})
	defer endObservation(1, observation.Args{})

	if upload.UploadedParts == nil {
		upload.UploadedParts = []int{}
	}

	id, _, err := basestore.ScanFirstInt(s.Store.Query(
		ctx,
		sqlf.Sprintf(
			insertUploadQuery,
			upload.Commit,
			upload.Root,
			upload.RepositoryID,
			upload.Indexer,
			upload.State,
			upload.NumParts,
			pq.Array(upload.UploadedParts),
			upload.UploadSize,
			upload.AssociatedIndexID,
		),
	))

	return id, err
}

const insertUploadQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:InsertUpload
INSERT INTO lsif_uploads (
	commit,
	root,
	repository_id,
	indexer,
	state,
	num_parts,
	uploaded_parts,
	upload_size,
	associated_index_id
) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
RETURNING id
`

// AddUploadPart adds the part index to the given upload's uploaded parts array. This method is idempotent
// (the resulting array is deduplicated on update).
func (s *Store) AddUploadPart(ctx context.Context, uploadID, partIndex int) (err error) {
	ctx, endObservation := s.operations.addUploadPart.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("uploadID", uploadID),
		log.Int("partIndex", partIndex),
	}})
	defer endObservation(1, observation.Args{})

	return s.Store.Exec(ctx, sqlf.Sprintf(addUploadPartQuery, partIndex, uploadID))
}

const addUploadPartQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:AddUploadPart
UPDATE lsif_uploads SET uploaded_parts = array(SELECT DISTINCT * FROM unnest(array_append(uploaded_parts, %s))) WHERE id = %s
`

// MarkQueued updates the state of the upload to queued and updates the upload size.
func (s *Store) MarkQueued(ctx context.Context, id int, uploadSize *int64) (err error) {
	ctx, endObservation := s.operations.markQueued.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("id", id),
	}})
	defer endObservation(1, observation.Args{})

	return s.Store.Exec(ctx, sqlf.Sprintf(markQueuedQuery, uploadSize, id))
}

const markQueuedQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:MarkQueued
UPDATE lsif_uploads SET state = 'queued', upload_size = %s WHERE id = %s
`

var uploadColumnsWithNullRank = []*sqlf.Query{
	sqlf.Sprintf("u.id"),
	sqlf.Sprintf("u.commit"),
	sqlf.Sprintf("u.root"),
	sqlf.Sprintf("EXISTS (SELECT 1 FROM lsif_uploads_visible_at_tip WHERE state != 'deleted' AND repository_id = u.repository_id AND upload_id = u.id) AS visible_at_tip"),
	sqlf.Sprintf("u.uploaded_at"),
	sqlf.Sprintf("u.state"),
	sqlf.Sprintf("u.failure_message"),
	sqlf.Sprintf("u.started_at"),
	sqlf.Sprintf("u.finished_at"),
	sqlf.Sprintf("u.process_after"),
	sqlf.Sprintf("u.num_resets"),
	sqlf.Sprintf("u.num_failures"),
	sqlf.Sprintf("u.repository_id"),
	sqlf.Sprintf(`u.repository_name`),
	sqlf.Sprintf("u.indexer"),
	sqlf.Sprintf("u.num_parts"),
	sqlf.Sprintf("u.uploaded_parts"),
	sqlf.Sprintf("u.upload_size"),
	sqlf.Sprintf("u.associated_index_id"),
	sqlf.Sprintf("NULL"),
}

// DeleteUploadByID deletes an upload by its identifier. This method returns a true-valued flag if a record
// was deleted. The associated repository will be marked as dirty so that its commit graph will be updated in
// the background.
func (s *Store) DeleteUploadByID(ctx context.Context, id int) (_ bool, err error) {
	ctx, endObservation := s.operations.deleteUploadByID.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("id", id),
	}})
	defer endObservation(1, observation.Args{})

	tx, err := s.transact(ctx)
	if err != nil {
		return false, err
	}
	defer func() { err = tx.Done(err) }()

	repositoryID, deleted, err := basestore.ScanFirstInt(tx.Store.Query(ctx, sqlf.Sprintf(deleteUploadByIDQuery, id)))
	if err != nil {
		return false, err
	}
	if !deleted {
		return false, nil
	}

	if err := tx.MarkRepositoryAsDirty(ctx, repositoryID); err != nil {
		return false, err
	}

	return true, nil
}

const deleteUploadByIDQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:DeleteUploadByID
UPDATE lsif_uploads SET state = 'deleted' WHERE id = %s RETURNING repository_id
`

// DeletedRepositoryGracePeriod is the minimum allowable duration between a repo deletion
// and the upload and index records for that repository being deleted.
const DeletedRepositoryGracePeriod = time.Minute * 30

// DeleteUploadsWithoutRepository deletes uploads associated with repositories that were deleted at least
// DeletedRepositoryGracePeriod ago. This returns the repository identifier mapped to the number of uploads
// that were removed for that repository.
func (s *Store) DeleteUploadsWithoutRepository(ctx context.Context, now time.Time) (_ map[int]int, err error) {
	ctx, endObservation := s.operations.deleteUploadsWithoutRepository.With(ctx, &err, observation.Args{LogFields: []log.Field{}})
	defer endObservation(1, observation.Args{})

	return scanCounts(s.Store.Query(ctx, sqlf.Sprintf(deleteUploadsWithoutRepositoryQuery, now.UTC(), DeletedRepositoryGracePeriod/time.Second)))
}

const deleteUploadsWithoutRepositoryQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:DeleteUploadsWithoutRepository
-- TODO(efritz) - this would benefit from an index on repository_id. We currently have
-- a similar one on this index, but only for uploads that are completed or visible at tip.
WITH deleted_repos AS (
	SELECT r.id AS id FROM repo r
	WHERE
		%s - r.deleted_at >= %s * interval '1 second' AND
		EXISTS (SELECT 1 from lsif_uploads u WHERE u.repository_id = r.id)
),
deleted_uploads AS (
	UPDATE lsif_uploads u
	SET state = 'deleted'
	WHERE u.repository_id IN (SELECT id FROM deleted_repos)
	RETURNING u.id, u.repository_id
)
SELECT d.repository_id, COUNT(*) FROM deleted_uploads d GROUP BY d.repository_id
`

// HardDeleteUploadByID deletes the upload record with the given identifier.
func (s *Store) HardDeleteUploadByID(ctx context.Context, ids ...int) (err error) {
	ctx, endObservation := s.operations.hardDeleteUploadByID.With(ctx, &err, observation.Args{LogFields: []log.Field{}})
	defer endObservation(1, observation.Args{})

	if len(ids) == 0 {
		return nil
	}

	var idQueries []*sqlf.Query
	for _, id := range ids {
		idQueries = append(idQueries, sqlf.Sprintf("%s", id))
	}

	return s.Store.Exec(ctx, sqlf.Sprintf(hardDeleteUploadByIDQuery, sqlf.Join(idQueries, ", ")))
}

const hardDeleteUploadByIDQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:HardDeleteUploadByID
DELETE FROM lsif_uploads WHERE id IN (%s)
`

// SoftDeleteOldUploads marks uploads older than the given age that are not visible at the tip of the default branch
// as deleted. The associated repositories will be marked as dirty so that their commit graphs are updated in the
// background.
func (s *Store) SoftDeleteOldUploads(ctx context.Context, maxAge time.Duration, now time.Time) (count int, err error) {
	ctx, endObservation := s.operations.softDeleteOldUploads.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("maxAge", maxAge.String()),
	}})
	defer endObservation(1, observation.Args{})

	tx, err := s.transact(ctx)
	if err != nil {
		return 0, err
	}
	defer func() { err = tx.Done(err) }()

	seconds := strconv.Itoa(int(maxAge / time.Second))
	repositoryIDs, err := scanCounts(tx.Store.Query(ctx, sqlf.Sprintf(softDeleteOldUploadsQuery, now, seconds, now, seconds)))
	if err != nil {
		return 0, err
	}

	for repositoryID, numUpdated := range repositoryIDs {
		if err := tx.MarkRepositoryAsDirty(ctx, repositoryID); err != nil {
			return 0, err
		}

		count += numUpdated
	}

	return count, nil
}

const softDeleteOldUploadsQuery = `
-- source: enterprise/internal/codeintel/stores/dbstore/uploads.go:SoftDeleteOldUploads
WITH u AS (
	UPDATE lsif_uploads u
		SET state = 'deleted'
		WHERE
			(
				%s - u.finished_at > (%s || ' second')::interval OR
				(u.finished_at IS NULL AND %s - u.uploaded_at > (%s || ' second')::interval)
			) AND
				u.id NOT IN (SELECT uv.upload_id FROM lsif_uploads_visible_at_tip uv WHERE uv.repository_id = u.repository_id)
		RETURNING id, repository_id
)
SELECT u.repository_id, count(*) FROM u GROUP BY u.repository_id
`
