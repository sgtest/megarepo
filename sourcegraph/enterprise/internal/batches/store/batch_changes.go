package store

import (
	"context"
	"strconv"

	"github.com/keegancsmith/sqlf"
	"github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/go-diff/diff"

	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// batchChangeColumns are used by the batch change related Store methods to insert,
// update and query batches.
var batchChangeColumns = []*sqlf.Query{
	sqlf.Sprintf("batch_changes.id"),
	sqlf.Sprintf("batch_changes.name"),
	sqlf.Sprintf("batch_changes.description"),
	sqlf.Sprintf("batch_changes.creator_id"),
	sqlf.Sprintf("batch_changes.last_applier_id"),
	sqlf.Sprintf("batch_changes.last_applied_at"),
	sqlf.Sprintf("batch_changes.namespace_user_id"),
	sqlf.Sprintf("batch_changes.namespace_org_id"),
	sqlf.Sprintf("batch_changes.created_at"),
	sqlf.Sprintf("batch_changes.updated_at"),
	sqlf.Sprintf("batch_changes.closed_at"),
	sqlf.Sprintf("batch_changes.batch_spec_id"),
}

// batchChangeInsertColumns is the list of batch changes columns that are
// modified in CreateBatchChange and UpdateBatchChange.
// update and query batches.
var batchChangeInsertColumns = []*sqlf.Query{
	sqlf.Sprintf("name"),
	sqlf.Sprintf("description"),
	sqlf.Sprintf("creator_id"),
	sqlf.Sprintf("last_applier_id"),
	sqlf.Sprintf("last_applied_at"),
	sqlf.Sprintf("namespace_user_id"),
	sqlf.Sprintf("namespace_org_id"),
	sqlf.Sprintf("created_at"),
	sqlf.Sprintf("updated_at"),
	sqlf.Sprintf("closed_at"),
	sqlf.Sprintf("batch_spec_id"),
}

func (s *Store) UpsertBatchChange(ctx context.Context, c *btypes.BatchChange) (err error) {
	ctx, _, endObservation := s.operations.upsertBatchChange.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	q := s.upsertBatchChangeQuery(c)

	return s.query(ctx, q, func(sc dbutil.Scanner) (err error) {
		return scanBatchChange(c, sc)
	})
}

var upsertBatchChangeQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:UpsertBatchChange
INSERT INTO batch_changes (%s)
VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
ON CONFLICT (%s) WHERE %s
DO UPDATE SET
(%s) = (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
RETURNING %s
`

func (s *Store) upsertBatchChangeQuery(c *btypes.BatchChange) *sqlf.Query {
	if c.CreatedAt.IsZero() {
		c.CreatedAt = s.now()
	}

	if c.UpdatedAt.IsZero() {
		c.UpdatedAt = c.CreatedAt
	}

	conflictTarget := []*sqlf.Query{sqlf.Sprintf("name")}
	predicate := sqlf.Sprintf("TRUE")

	if c.NamespaceUserID != 0 {
		conflictTarget = append(conflictTarget, sqlf.Sprintf("namespace_user_id"))
		predicate = sqlf.Sprintf("namespace_user_id IS NOT NULL")
	}

	if c.NamespaceOrgID != 0 {
		conflictTarget = append(conflictTarget, sqlf.Sprintf("namespace_org_id"))
		predicate = sqlf.Sprintf("namespace_org_id IS NOT NULL")
	}

	return sqlf.Sprintf(
		upsertBatchChangeQueryFmtstr,
		sqlf.Join(batchChangeInsertColumns, ", "),
		c.Name,
		c.Description,
		nullInt32Column(c.CreatorID),
		nullInt32Column(c.LastApplierID),
		nullTimeColumn(c.LastAppliedAt),
		nullInt32Column(c.NamespaceUserID),
		nullInt32Column(c.NamespaceOrgID),
		c.CreatedAt,
		c.UpdatedAt,
		nullTimeColumn(c.ClosedAt),
		c.BatchSpecID,
		sqlf.Join(conflictTarget, ", "),
		predicate,
		sqlf.Join(batchChangeInsertColumns, ", "),
		c.Name,
		c.Description,
		nullInt32Column(c.CreatorID),
		nullInt32Column(c.LastApplierID),
		nullTimeColumn(c.LastAppliedAt),
		nullInt32Column(c.NamespaceUserID),
		nullInt32Column(c.NamespaceOrgID),
		c.CreatedAt,
		c.UpdatedAt,
		nullTimeColumn(c.ClosedAt),
		c.BatchSpecID,
		sqlf.Join(batchChangeColumns, ", "),
	)
}

// CreateBatchChange creates the given batch change.
func (s *Store) CreateBatchChange(ctx context.Context, c *btypes.BatchChange) (err error) {
	ctx, _, endObservation := s.operations.createBatchChange.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	q := s.createBatchChangeQuery(c)

	return s.query(ctx, q, func(sc dbutil.Scanner) (err error) {
		return scanBatchChange(c, sc)
	})
}

var createBatchChangeQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:CreateBatchChange
INSERT INTO batch_changes (%s)
VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
RETURNING %s
`

func (s *Store) createBatchChangeQuery(c *btypes.BatchChange) *sqlf.Query {
	if c.CreatedAt.IsZero() {
		c.CreatedAt = s.now()
	}

	if c.UpdatedAt.IsZero() {
		c.UpdatedAt = c.CreatedAt
	}

	return sqlf.Sprintf(
		createBatchChangeQueryFmtstr,
		sqlf.Join(batchChangeInsertColumns, ", "),
		c.Name,
		c.Description,
		nullInt32Column(c.CreatorID),
		nullInt32Column(c.LastApplierID),
		nullTimeColumn(c.LastAppliedAt),
		nullInt32Column(c.NamespaceUserID),
		nullInt32Column(c.NamespaceOrgID),
		c.CreatedAt,
		c.UpdatedAt,
		nullTimeColumn(c.ClosedAt),
		c.BatchSpecID,
		sqlf.Join(batchChangeColumns, ", "),
	)
}

// UpdateBatchChange updates the given bach change.
func (s *Store) UpdateBatchChange(ctx context.Context, c *btypes.BatchChange) (err error) {
	ctx, _, endObservation := s.operations.updateBatchChange.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("ID", int(c.ID)),
	}})
	defer endObservation(1, observation.Args{})

	q := s.updateBatchChangeQuery(c)

	return s.query(ctx, q, func(sc dbutil.Scanner) (err error) { return scanBatchChange(c, sc) })
}

var updateBatchChangeQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:UpdateBatchChange
UPDATE batch_changes
SET (%s) = (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
WHERE id = %s
RETURNING %s
`

func (s *Store) updateBatchChangeQuery(c *btypes.BatchChange) *sqlf.Query {
	c.UpdatedAt = s.now()

	return sqlf.Sprintf(
		updateBatchChangeQueryFmtstr,
		sqlf.Join(batchChangeInsertColumns, ", "),
		c.Name,
		c.Description,
		nullInt32Column(c.CreatorID),
		nullInt32Column(c.LastApplierID),
		nullTimeColumn(c.LastAppliedAt),
		nullInt32Column(c.NamespaceUserID),
		nullInt32Column(c.NamespaceOrgID),
		c.CreatedAt,
		c.UpdatedAt,
		nullTimeColumn(c.ClosedAt),
		c.BatchSpecID,
		c.ID,
		sqlf.Join(batchChangeColumns, ", "),
	)
}

// DeleteBatchChange deletes the batch change with the given ID.
func (s *Store) DeleteBatchChange(ctx context.Context, id int64) (err error) {
	ctx, _, endObservation := s.operations.deleteBatchChange.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("ID", int(id)),
	}})
	defer endObservation(1, observation.Args{})

	return s.Store.Exec(ctx, sqlf.Sprintf(deleteBatchChangeQueryFmtstr, id))
}

var deleteBatchChangeQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:DeleteBatchChange
DELETE FROM batch_changes WHERE id = %s
`

// CountBatchChangesOpts captures the query options needed for
// counting batches.
type CountBatchChangesOpts struct {
	ChangesetID int64
	States      []btypes.BatchChangeState
	RepoID      api.RepoID

	CreatorID int32

	NamespaceUserID int32
	NamespaceOrgID  int32

	ExcludeDraftsNotOwnedByUserID int32
}

// CountBatchChanges returns the number of batch changes in the database.
func (s *Store) CountBatchChanges(ctx context.Context, opts CountBatchChangesOpts) (count int, err error) {
	ctx, _, endObservation := s.operations.countBatchChanges.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	repoAuthzConds, err := database.AuthzQueryConds(ctx, database.NewDBWith(s.logger, s))
	if err != nil {
		return 0, errors.Wrap(err, "CountBatchChanges generating authz query conds")
	}

	return s.queryCount(ctx, countBatchChangesQuery(&opts, repoAuthzConds))
}

var countBatchChangesQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:CountBatchChanges
SELECT COUNT(batch_changes.id)
FROM batch_changes
%s
WHERE %s
`

func countBatchChangesQuery(opts *CountBatchChangesOpts, repoAuthzConds *sqlf.Query) *sqlf.Query {
	joins := []*sqlf.Query{
		sqlf.Sprintf("LEFT JOIN users namespace_user ON batch_changes.namespace_user_id = namespace_user.id"),
		sqlf.Sprintf("LEFT JOIN orgs namespace_org ON batch_changes.namespace_org_id = namespace_org.id"),
	}
	preds := []*sqlf.Query{
		sqlf.Sprintf("namespace_user.deleted_at IS NULL"),
		sqlf.Sprintf("namespace_org.deleted_at IS NULL"),
	}

	if opts.ChangesetID != 0 {
		joins = append(joins, sqlf.Sprintf("INNER JOIN changesets ON changesets.batch_change_ids ? batch_changes.id::TEXT"))
		preds = append(preds, sqlf.Sprintf("changesets.id = %s", opts.ChangesetID))
	}

	if len(opts.States) > 0 {
		stateConds := []*sqlf.Query{}
		for _, state := range opts.States {
			switch state {
			case btypes.BatchChangeStateOpen:
				stateConds = append(stateConds, sqlf.Sprintf("batch_changes.closed_at IS NULL AND batch_changes.last_applied_at IS NOT NULL"))
			case btypes.BatchChangeStateClosed:
				stateConds = append(stateConds, sqlf.Sprintf("batch_changes.closed_at IS NOT NULL"))
			case btypes.BatchChangeStateDraft:
				stateConds = append(stateConds, sqlf.Sprintf("batch_changes.last_applied_at IS NULL AND batch_changes.closed_at IS NULL"))
			}
		}
		if len(stateConds) > 0 {
			preds = append(preds, sqlf.Sprintf("(%s)", sqlf.Join(stateConds, "OR")))
		}
	}

	if opts.CreatorID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.creator_id = %d", opts.CreatorID))
	}

	if opts.NamespaceUserID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.namespace_user_id = %s", opts.NamespaceUserID))

		// If it's not my namespace and I can't see other users' drafts, filter out
		// unapplied (draft) batch changes from this list.
		if opts.ExcludeDraftsNotOwnedByUserID != 0 && opts.ExcludeDraftsNotOwnedByUserID != opts.NamespaceUserID {
			preds = append(preds, sqlf.Sprintf("batch_changes.last_applied_at IS NOT NULL"))
		}
		// For batch changes filtered by org namespace, or not filtered by namespace at
		// all, if I can't see other users' drafts, filter out unapplied (draft) batch
		// changes except those that I authored the batch spec of from this list.
	} else if opts.ExcludeDraftsNotOwnedByUserID != 0 {
		cond := sqlf.Sprintf(`(batch_changes.last_applied_at IS NOT NULL
		OR
		EXISTS (SELECT 1 FROM batch_specs WHERE batch_specs.id = batch_changes.batch_spec_id AND batch_specs.user_id = %s))
		`, opts.ExcludeDraftsNotOwnedByUserID)
		preds = append(preds, cond)
	}

	if opts.NamespaceOrgID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.namespace_org_id = %s", opts.NamespaceOrgID))
	}

	if opts.RepoID != 0 {
		preds = append(preds, sqlf.Sprintf(`EXISTS(
			SELECT * FROM changesets
			INNER JOIN repo ON changesets.repo_id = repo.id
			WHERE
				changesets.batch_change_ids ? batch_changes.id::TEXT AND
				changesets.repo_id = %s AND
				repo.deleted_at IS NULL AND
				-- authz conditions:
				%s
		)`, opts.RepoID, repoAuthzConds))
	}

	if len(preds) == 0 {
		preds = append(preds, sqlf.Sprintf("TRUE"))
	}

	return sqlf.Sprintf(countBatchChangesQueryFmtstr, sqlf.Join(joins, "\n"), sqlf.Join(preds, "\n AND "))
}

// GetBatchChangeOpts captures the query options needed for getting a batch change
type GetBatchChangeOpts struct {
	ID int64

	NamespaceUserID int32
	NamespaceOrgID  int32

	BatchSpecID int64
	Name        string
}

// GetBatchChange gets a batch change matching the given options.
func (s *Store) GetBatchChange(ctx context.Context, opts GetBatchChangeOpts) (bc *btypes.BatchChange, err error) {
	ctx, _, endObservation := s.operations.getBatchChange.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("ID", int(opts.ID)),
	}})
	defer endObservation(1, observation.Args{})

	q := getBatchChangeQuery(&opts)

	var c btypes.BatchChange
	err = s.query(ctx, q, func(sc dbutil.Scanner) error {
		return scanBatchChange(&c, sc)
	})
	if err != nil {
		return nil, err
	}

	if c.ID == 0 {
		return nil, ErrNoResults
	}

	return &c, nil
}

var getBatchChangesQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:GetBatchChange
SELECT %s FROM batch_changes
LEFT JOIN users namespace_user ON batch_changes.namespace_user_id = namespace_user.id
LEFT JOIN orgs  namespace_org  ON batch_changes.namespace_org_id = namespace_org.id
WHERE %s
LIMIT 1
`

func getBatchChangeQuery(opts *GetBatchChangeOpts) *sqlf.Query {
	preds := []*sqlf.Query{
		sqlf.Sprintf("namespace_user.deleted_at IS NULL"),
		sqlf.Sprintf("namespace_org.deleted_at IS NULL"),
	}
	if opts.ID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.id = %s", opts.ID))
	}

	if opts.BatchSpecID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.batch_spec_id = %s", opts.BatchSpecID))
	}

	if opts.NamespaceUserID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.namespace_user_id = %s", opts.NamespaceUserID))
	}

	if opts.NamespaceOrgID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.namespace_org_id = %s", opts.NamespaceOrgID))
	}

	if opts.Name != "" {
		preds = append(preds, sqlf.Sprintf("batch_changes.name = %s", opts.Name))
	}

	if len(preds) == 0 {
		preds = append(preds, sqlf.Sprintf("TRUE"))
	}

	return sqlf.Sprintf(
		getBatchChangesQueryFmtstr,
		sqlf.Join(batchChangeColumns, ", "),
		sqlf.Join(preds, "\n AND "),
	)
}

type GetBatchChangeDiffStatOpts struct {
	BatchChangeID int64
}

func (s *Store) GetBatchChangeDiffStat(ctx context.Context, opts GetBatchChangeDiffStatOpts) (stat *diff.Stat, err error) {
	ctx, _, endObservation := s.operations.getBatchChangeDiffStat.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("batchChangeID", int(opts.BatchChangeID)),
	}})
	defer endObservation(1, observation.Args{})

	authzConds, err := database.AuthzQueryConds(ctx, database.NewDBWith(s.logger, s))
	if err != nil {
		return nil, errors.Wrap(err, "GetBatchChangeDiffStat generating authz query conds")
	}
	q := getBatchChangeDiffStatQuery(opts, authzConds)

	var diffStat diff.Stat
	err = s.query(ctx, q, func(sc dbutil.Scanner) error {
		return sc.Scan(&diffStat.Added, &diffStat.Changed, &diffStat.Deleted)
	})
	if err != nil {
		return nil, err
	}

	return &diffStat, nil
}

var getBatchChangeDiffStatQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:GetBatchChangeDiffStat
SELECT
	COALESCE(SUM(diff_stat_added), 0) AS added,
	COALESCE(SUM(diff_stat_changed), 0) AS changed,
	COALESCE(SUM(diff_stat_deleted), 0) AS deleted
FROM
	changesets
INNER JOIN repo ON changesets.repo_id = repo.id
WHERE
	changesets.batch_change_ids ? %s AND
	repo.deleted_at IS NULL AND
	-- authz conditions:
	%s
`

func getBatchChangeDiffStatQuery(opts GetBatchChangeDiffStatOpts, authzConds *sqlf.Query) *sqlf.Query {
	return sqlf.Sprintf(getBatchChangeDiffStatQueryFmtstr, strconv.Itoa(int(opts.BatchChangeID)), authzConds)
}

func (s *Store) GetRepoDiffStat(ctx context.Context, repoID api.RepoID) (stat *diff.Stat, err error) {
	ctx, _, endObservation := s.operations.getRepoDiffStat.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("repoID", int(repoID)),
	}})
	defer endObservation(1, observation.Args{})

	authzConds, err := database.AuthzQueryConds(ctx, database.NewDBWith(s.logger, s))
	if err != nil {
		return nil, errors.Wrap(err, "GetRepoDiffStat generating authz query conds")
	}
	q := getRepoDiffStatQuery(int64(repoID), authzConds)

	var diffStat diff.Stat
	err = s.query(ctx, q, func(sc dbutil.Scanner) error {
		return sc.Scan(&diffStat.Added, &diffStat.Changed, &diffStat.Deleted)
	})
	if err != nil {
		return nil, err
	}

	return &diffStat, nil
}

var getRepoDiffStatQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:GetRepoDiffStat
SELECT
	COALESCE(SUM(diff_stat_added), 0) AS added,
	COALESCE(SUM(diff_stat_changed), 0) AS changed,
	COALESCE(SUM(diff_stat_deleted), 0) AS deleted
FROM changesets
INNER JOIN repo ON changesets.repo_id = repo.id
WHERE
	changesets.repo_id = %s AND
	repo.deleted_at IS NULL AND
	-- authz conditions:
	%s
`

func getRepoDiffStatQuery(repoID int64, authzConds *sqlf.Query) *sqlf.Query {
	return sqlf.Sprintf(getRepoDiffStatQueryFmtstr, repoID, authzConds)
}

// ListBatchChangesOpts captures the query options needed for
// listing batches.
type ListBatchChangesOpts struct {
	LimitOpts
	ChangesetID int64
	Cursor      int64
	States      []btypes.BatchChangeState

	CreatorID int32

	NamespaceUserID int32
	NamespaceOrgID  int32

	RepoID api.RepoID

	ExcludeDraftsNotOwnedByUserID int32
}

// ListBatchChanges lists batch changes with the given filters.
func (s *Store) ListBatchChanges(ctx context.Context, opts ListBatchChangesOpts) (cs []*btypes.BatchChange, next int64, err error) {
	ctx, _, endObservation := s.operations.listBatchChanges.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	repoAuthzConds, err := database.AuthzQueryConds(ctx, database.NewDBWith(s.logger, s))
	if err != nil {
		return nil, 0, errors.Wrap(err, "ListBatchChanges generating authz query conds")
	}
	q := listBatchChangesQuery(&opts, repoAuthzConds)

	cs = make([]*btypes.BatchChange, 0, opts.DBLimit())
	err = s.query(ctx, q, func(sc dbutil.Scanner) error {
		var c btypes.BatchChange
		if err := scanBatchChange(&c, sc); err != nil {
			return err
		}
		cs = append(cs, &c)
		return nil
	})

	if opts.Limit != 0 && len(cs) == opts.DBLimit() {
		next = cs[len(cs)-1].ID
		cs = cs[:len(cs)-1]
	}

	return cs, next, err
}

var listBatchChangesQueryFmtstr = `
-- source: enterprise/internal/batches/store/batch_changes.go:ListBatchChanges
SELECT %s FROM batch_changes
%s
WHERE %s
ORDER BY id DESC
`

func listBatchChangesQuery(opts *ListBatchChangesOpts, repoAuthzConds *sqlf.Query) *sqlf.Query {
	joins := []*sqlf.Query{
		sqlf.Sprintf("LEFT JOIN users namespace_user ON batch_changes.namespace_user_id = namespace_user.id"),
		sqlf.Sprintf("LEFT JOIN orgs namespace_org ON batch_changes.namespace_org_id = namespace_org.id"),
	}
	preds := []*sqlf.Query{
		sqlf.Sprintf("namespace_user.deleted_at IS NULL"),
		sqlf.Sprintf("namespace_org.deleted_at IS NULL"),
	}

	if opts.Cursor != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.id <= %s", opts.Cursor))
	}

	if opts.ChangesetID != 0 {
		joins = append(joins, sqlf.Sprintf("INNER JOIN changesets ON changesets.batch_change_ids ? batch_changes.id::TEXT"))
		preds = append(preds, sqlf.Sprintf("changesets.id = %s", opts.ChangesetID))
	}

	if len(opts.States) > 0 {
		stateConds := []*sqlf.Query{}
		for i := 0; i < len(opts.States); i++ {
			switch opts.States[i] {
			case btypes.BatchChangeStateOpen:
				stateConds = append(stateConds, sqlf.Sprintf("batch_changes.closed_at IS NULL AND batch_changes.last_applied_at IS NOT NULL"))
			case btypes.BatchChangeStateClosed:
				stateConds = append(stateConds, sqlf.Sprintf("batch_changes.closed_at IS NOT NULL"))
			case btypes.BatchChangeStateDraft:
				stateConds = append(stateConds, sqlf.Sprintf("batch_changes.last_applied_at IS NULL AND batch_changes.closed_at IS NULL"))
			}
		}
		if len(stateConds) > 0 {
			preds = append(preds, sqlf.Sprintf("(%s)", sqlf.Join(stateConds, "OR")))
		}
	}

	if opts.CreatorID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.creator_id = %d", opts.CreatorID))
	}

	if opts.NamespaceUserID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.namespace_user_id = %s", opts.NamespaceUserID))
		// If it's not my namespace and I can't see other users' drafts, filter out
		// unapplied (draft) batch changes from this list.
		if opts.ExcludeDraftsNotOwnedByUserID != 0 && opts.ExcludeDraftsNotOwnedByUserID != opts.NamespaceUserID {
			preds = append(preds, sqlf.Sprintf("batch_changes.last_applied_at IS NOT NULL"))
		}
		// For batch changes filtered by org namespace, or not filtered by namespace at
		// all, if I can't see other users' drafts, filter out unapplied (draft) batch
		// changes except those that I authored the batch spec of from this list.
	} else if opts.ExcludeDraftsNotOwnedByUserID != 0 {
		cond := sqlf.Sprintf(`(batch_changes.last_applied_at IS NOT NULL
		OR
		EXISTS (SELECT 1 FROM batch_specs WHERE batch_specs.id = batch_changes.batch_spec_id AND batch_specs.user_id = %s))
		`, opts.ExcludeDraftsNotOwnedByUserID)
		preds = append(preds, cond)
	}

	if opts.NamespaceOrgID != 0 {
		preds = append(preds, sqlf.Sprintf("batch_changes.namespace_org_id = %s", opts.NamespaceOrgID))
	}

	if opts.RepoID != 0 {
		preds = append(preds, sqlf.Sprintf(`EXISTS(
			SELECT * FROM changesets
			INNER JOIN repo ON changesets.repo_id = repo.id
			WHERE
				changesets.batch_change_ids ? batch_changes.id::TEXT AND
				changesets.repo_id = %s AND
				repo.deleted_at IS NULL AND
				-- authz conditions:
				%s
		)`, opts.RepoID, repoAuthzConds))
	}

	if len(preds) == 0 {
		preds = append(preds, sqlf.Sprintf("TRUE"))
	}

	return sqlf.Sprintf(
		listBatchChangesQueryFmtstr+opts.LimitOpts.ToDB(),
		sqlf.Join(batchChangeColumns, ", "),
		sqlf.Join(joins, "\n"),
		sqlf.Join(preds, "\n AND "),
	)
}

func scanBatchChange(c *btypes.BatchChange, s dbutil.Scanner) error {
	return s.Scan(
		&c.ID,
		&c.Name,
		&dbutil.NullString{S: &c.Description},
		&dbutil.NullInt32{N: &c.CreatorID},
		&dbutil.NullInt32{N: &c.LastApplierID},
		&dbutil.NullTime{Time: &c.LastAppliedAt},
		&dbutil.NullInt32{N: &c.NamespaceUserID},
		&dbutil.NullInt32{N: &c.NamespaceOrgID},
		&c.CreatedAt,
		&c.UpdatedAt,
		&dbutil.NullTime{Time: &c.ClosedAt},
		&c.BatchSpecID,
	)
}
