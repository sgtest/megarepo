package store

import (
	"context"
	"database/sql"

	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/types"
)

// scanPackageReferences scans a slice of package references from the return value of `*store.query`.
func scanPackageReferences(rows *sql.Rows, queryErr error) (_ []types.PackageReference, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	var references []types.PackageReference
	for rows.Next() {
		var reference types.PackageReference
		if err := rows.Scan(
			&reference.DumpID,
			&reference.Scheme,
			&reference.Name,
			&reference.Version,
			&reference.Filter,
		); err != nil {
			return nil, err
		}

		references = append(references, reference)
	}

	return references, nil
}

// SameRepoPager returns a ReferencePager for dumps that belong to the given repository and commit and reference the package with the
// given scheme, name, and version.
func (s *store) SameRepoPager(ctx context.Context, repositoryID int, commit, scheme, name, version string, limit int) (_ int, _ ReferencePager, err error) {
	tx, started, err := s.transact(ctx)
	if err != nil {
		return 0, nil, err
	}

	done := noopDoneFunc
	if started {
		done = tx.Done
	}

	visibleIDs, err := scanInts(tx.query(
		ctx,
		withBidirectionalLineage(`SELECT id FROM visible_ids`, repositoryID, commit),
	))
	if err != nil {
		return 0, nil, done(err)
	}
	if len(visibleIDs) == 0 {
		return 0, newReferencePager(noopPageFromOffsetFunc, done), nil
	}

	conds := []*sqlf.Query{
		sqlf.Sprintf("r.scheme = %s", scheme),
		sqlf.Sprintf("r.name = %s", name),
		sqlf.Sprintf("r.version = %s", version),
		sqlf.Sprintf("r.dump_id IN (%s)", sqlf.Join(intsToQueries(visibleIDs), ", ")),
	}

	totalCount, _, err := scanFirstInt(tx.query(
		ctx,
		sqlf.Sprintf(`SELECT COUNT(*) FROM lsif_references r WHERE %s`, sqlf.Join(conds, " AND ")),
	))
	if err != nil {
		return 0, nil, done(err)
	}

	pageFromOffset := func(ctx context.Context, offset int) ([]types.PackageReference, error) {
		return scanPackageReferences(tx.query(
			ctx,
			sqlf.Sprintf(`
				SELECT d.id, r.scheme, r.name, r.version, r.filter FROM lsif_references r
				LEFT JOIN lsif_dumps_with_repository_name d ON d.id = r.dump_id
				WHERE %s ORDER BY d.root LIMIT %d OFFSET %d
			`, sqlf.Join(conds, " AND "), limit, offset),
		))
	}

	return totalCount, newReferencePager(pageFromOffset, done), nil
}

// PackageReferencePager returns a ReferencePager for dumps that belong to a remote repository (distinct from the given repository id)
// and reference the package with the given scheme, name, and version. All resulting dumps are visible at the tip of their repository's
// default branch.
func (s *store) PackageReferencePager(ctx context.Context, scheme, name, version string, repositoryID, limit int) (_ int, _ ReferencePager, err error) {
	tx, started, err := s.transact(ctx)
	if err != nil {
		return 0, nil, err
	}

	done := noopDoneFunc
	if started {
		done = tx.Done
	}

	conds := []*sqlf.Query{
		sqlf.Sprintf("r.scheme = %s", scheme),
		sqlf.Sprintf("r.name = %s", name),
		sqlf.Sprintf("r.version = %s", version),
		sqlf.Sprintf("d.repository_id != %s", repositoryID),
		sqlf.Sprintf("d.visible_at_tip = true"),
	}

	totalCount, _, err := scanFirstInt(tx.query(
		ctx,
		sqlf.Sprintf(`
			SELECT COUNT(*) FROM lsif_references r
			LEFT JOIN lsif_dumps_with_repository_name d ON d.id = r.dump_id
			WHERE %s
		`, sqlf.Join(conds, " AND ")),
	))
	if err != nil {
		return 0, nil, done(err)
	}

	pageFromOffset := func(ctx context.Context, offset int) ([]types.PackageReference, error) {
		return scanPackageReferences(tx.query(ctx, sqlf.Sprintf(`
			SELECT d.id, r.scheme, r.name, r.version, r.filter FROM lsif_references r
			LEFT JOIN lsif_dumps_with_repository_name d ON d.id = r.dump_id
			WHERE %s ORDER BY d.repository_id, d.root LIMIT %d OFFSET %d
		`, sqlf.Join(conds, " AND "), limit, offset)))
	}

	return totalCount, newReferencePager(pageFromOffset, done), nil
}

// UpdatePackageReferences inserts reference data tied to the given upload.
func (s *store) UpdatePackageReferences(ctx context.Context, references []types.PackageReference) (err error) {
	if len(references) == 0 {
		return nil
	}

	var values []*sqlf.Query
	for _, r := range references {
		values = append(values, sqlf.Sprintf("(%s, %s, %s, %s, %s)", r.DumpID, r.Scheme, r.Name, r.Version, r.Filter))
	}

	return s.queryForEffect(ctx, sqlf.Sprintf(`
		INSERT INTO lsif_references (dump_id, scheme, name, version, filter)
		VALUES %s
	`, sqlf.Join(values, ",")))
}
