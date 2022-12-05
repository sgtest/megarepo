package store

import (
	"context"
	"database/sql"
	"fmt"
	"sort"
	"strings"

	"github.com/jackc/pgconn"
	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/locker"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/definition"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/shared"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Store struct {
	*basestore.Store
	schemaName string
	operations *Operations
}

func NewWithDB(observationCtx *observation.Context, db *sql.DB, migrationsTable string) *Store {
	operations := NewOperations(observationCtx)
	return &Store{
		Store:      basestore.NewWithHandle(basestore.NewHandleWithDB(observationCtx.Logger, db, sql.TxOptions{})),
		schemaName: migrationsTable,
		operations: operations,
	}
}

func (s *Store) With(other basestore.ShareableStore) *Store {
	return &Store{
		Store:      s.Store.With(other),
		schemaName: s.schemaName,
		operations: s.operations,
	}
}

func (s *Store) Transact(ctx context.Context) (*Store, error) {
	txBase, err := s.Store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return &Store{
		Store:      txBase,
		schemaName: s.schemaName,
		operations: s.operations,
	}, nil
}

const currentMigrationLogSchemaVersion = 2

// EnsureSchemaTable creates the bookeeping tables required to track this schema
// if they do not already exist. If old versions of the tables exist, this method
// will attempt to update them in a backward-compatible manner.
func (s *Store) EnsureSchemaTable(ctx context.Context) (err error) {
	ctx, _, endObservation := s.operations.ensureSchemaTable.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	queries := []*sqlf.Query{
		sqlf.Sprintf(`CREATE TABLE IF NOT EXISTS migration_logs(id SERIAL PRIMARY KEY)`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS migration_logs_schema_version integer NOT NULL`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS schema text NOT NULL`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS version integer NOT NULL`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS up bool NOT NULL`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS started_at timestamptz NOT NULL`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS finished_at timestamptz`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS success boolean`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS error_message text`),
		sqlf.Sprintf(`ALTER TABLE migration_logs ADD COLUMN IF NOT EXISTS backfilled boolean NOT NULL DEFAULT FALSE`),
	}

	tx, err := s.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	for _, query := range queries {
		if err := tx.Exec(ctx, query); err != nil {
			return err
		}
	}

	return nil
}

// BackfillSchemaVersions adds "backfilled" rows into the migration_logs table to make instances
// upgraded from older versions work uniformly with instances booted from a newer version.
//
// Backfilling mainly addresses issues during upgrades and interacting with migration graph defined
// over multiple versions being stitched back together. The absence of a row in the migration_logs
// table either represents a migration that needs to be applied, or a migration defined in a version
// prior to the instance's first boot. Backfilling these records prevents the latter circumstance as
// being interpreted as the former.
func (s *Store) BackfillSchemaVersions(ctx context.Context) error {
	// Choose the lowest relevant version (most like the smallest squashed migration) that has
	// been successfully applied on this instance. We will be backfilling all ancestors of this
	// migration version given the stitched migration graph.
	version, ok, err := s.inferBackfillTarget(ctx)
	if err != nil {
		return err
	}
	if !ok {
		return nil
	}

	// Determine ancestors of the chosen root
	ancestorDefinitions, err := s.stitchedMigration().Definitions.Up(nil, []int{version})
	if err != nil {
		return err
	}

	// Write backfilled versions into migration_logs table
	ids := make([]int64, 0, len(ancestorDefinitions))
	for _, definition := range ancestorDefinitions {
		ids = append(ids, int64(definition.ID))
	}
	if err := s.Exec(ctx, sqlf.Sprintf(backfillSchemaVersionsQuery, currentMigrationLogSchemaVersion, s.schemaName, pq.Int64Array(ids))); err != nil {
		return err
	}

	return nil
}

const backfillSchemaVersionsQuery = `
WITH candidates AS (
	SELECT
		%s::integer AS migration_logs_schema_version,
		%s AS schema,
		version AS version,
		true AS up,
		NOW() AS started_at,
		NOW() AS finished_at,
		true AS success,
		true AS backfilled
	FROM (SELECT unnest(%s::integer[])) AS vs(version)
)
INSERT INTO migration_logs (
	migration_logs_schema_version,
	schema,
	version,
	up,
	started_at,
	finished_at,
	success,
	backfilled
)
SELECT c.* FROM candidates c
WHERE NOT EXISTS (
	SELECT 1 FROM migration_logs ml
	WHERE ml.schema = c.schema AND ml.version = c.version
)
`

func (s *Store) inferBackfillTarget(ctx context.Context) (int, bool, error) {
	if version, ok, err := s.inferbackfillTargetViaMigrationLogs(ctx); err != nil || ok {
		return version, ok, err
	}

	// Fallback to golang migrate, but only if there's no authoritative data
	if version, ok, err := s.inferBackfillTargetViaGolangMigrate(ctx); err != nil || ok {
		return version, ok, err
	}

	return 0, false, nil
}

// inferbackfillTargetViaMigrationLogs reads the migration_logs table and returns the smallest
// identifier of a migration that has at one point been squashed. We use the fact that any existing
// instance with data in this table will have applied _some_ squashed migration. Any migrations
// defined prior to this version will be backfilled.
func (s *Store) inferbackfillTargetViaMigrationLogs(ctx context.Context) (int, bool, error) {
	applied, _, _, err := s.Versions(ctx)
	if err != nil {
		return 0, false, err
	}
	if len(applied) == 0 {
		return 0, false, nil
	}

	boundsByRev := s.stitchedMigration().BoundsByRev

	// make lookup map of applied migration identifiers
	appliedMap := make(map[int]struct{}, len(applied))
	for _, id := range applied {
		appliedMap[id] = struct{}{}
	}

	// collect root identifiers that have been applied and sort them
	appliedRootIDs := make([]int, 0, len(boundsByRev))
	for _, bound := range boundsByRev {
		rootID := bound.RootID
		if rootID < 0 {
			// If we have a "virtual" migration with a negative identifier, switch our references
			// to the direct child (with the same identifier but positive). This migration should
			// be an existant migration in the graph prior to a squash/stitch operation.
			rootID = -rootID
		}

		if _, ok := appliedMap[rootID]; ok {
			appliedRootIDs = append(appliedRootIDs, rootID)
		}
	}
	sort.Ints(appliedRootIDs)

	if len(appliedRootIDs) == 0 {
		return 0, false, nil
	}
	return appliedRootIDs[0], true, nil
}

// inferBackfillTargetViaGolangMigrate reads the old .*schema_migrations table (if it exists) and
// returns the version number. Any migration defined prior to this version will be backfilled.
//
// DO NOT call this method from inside a transaction, otherwise the absence of this relation will
// cause a transaction rollback while this function returns a nil-valued error (hard to debug).
func (s *Store) inferBackfillTargetViaGolangMigrate(ctx context.Context) (int, bool, error) {
	version, ok, err := basestore.ScanFirstInt(s.Query(ctx, sqlf.Sprintf(inferBackfillTargetViaGolangMigrateQuery, quote(s.schemaName))))
	if err != nil && !isMissingRelation(err) {
		return 0, false, err
	}

	return version, ok, nil
}

const inferBackfillTargetViaGolangMigrateQuery = `
SELECT version::integer FROM %s WHERE NOT dirty
`

// stitchedMigration returns the stitched migration graph (upgrade metadata) that is related
// to this store's schema.
func (s *Store) stitchedMigration() shared.StitchedMigration {
	return shared.StitchedMigationsBySchemaName[humanizeSchemaName(s.schemaName)]
}

// Versions returns three sets of migration versions that, together, describe the current schema
// state. These states describe, respectively, the identifieers of all applied, pending, and failed
// migrations.
//
// A failed migration requires administrator attention. A pending migration may currently be
// in-progress, or may indicate that a migration was attempted but failed part way through.
func (s *Store) Versions(ctx context.Context) (appliedVersions, pendingVersions, failedVersions []int, err error) {
	ctx, _, endObservation := s.operations.versions.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	migrationLogs, err := scanMigrationLogs(s.Query(ctx, sqlf.Sprintf(versionsQuery, s.schemaName)))
	if err != nil {
		return nil, nil, nil, err
	}

	for _, migrationLog := range migrationLogs {
		if migrationLog.Success == nil {
			pendingVersions = append(pendingVersions, migrationLog.Version)
			continue
		}
		if !*migrationLog.Success {
			failedVersions = append(failedVersions, migrationLog.Version)
			continue
		}
		if migrationLog.Up {
			appliedVersions = append(appliedVersions, migrationLog.Version)
		}
	}

	return appliedVersions, pendingVersions, failedVersions, nil
}

const versionsQuery = `
WITH ranked_migration_logs AS (
	SELECT
		migration_logs.*,
		ROW_NUMBER() OVER (PARTITION BY version ORDER BY backfilled, started_at DESC) AS row_number
	FROM migration_logs
	WHERE schema = %s
)
SELECT
	schema,
	version,
	up,
	success
FROM ranked_migration_logs
WHERE row_number = 1
ORDER BY version
`

// TryLock attempts to create hold an advisory lock. This method returns a function that should be
// called once the lock should be released. This method accepts the current function's error output
// and wraps any additional errors that occur on close. Calling this method when the lock was not
// acquired will return the given error without modification (no-op). If this method returns true,
// the lock was acquired and false if the lock is currently held by another process.
//
// Note that we don't use the internal/database/locker package here as that uses transactionally
// scoped advisory locks. We want to be able to hold locks outside of transactions for migrations.
func (s *Store) TryLock(ctx context.Context) (_ bool, _ func(err error) error, err error) {
	key := s.lockKey()

	ctx, _, endObservation := s.operations.tryLock.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int32("key", key),
	}})
	defer endObservation(1, observation.Args{})

	locked, _, err := basestore.ScanFirstBool(s.Query(ctx, sqlf.Sprintf(`SELECT pg_try_advisory_lock(%s, %s)`, key, 0)))
	if err != nil {
		return false, nil, err
	}

	close := func(err error) error {
		if locked {
			if unlockErr := s.Exec(ctx, sqlf.Sprintf(`SELECT pg_advisory_unlock(%s, %s)`, key, 0)); unlockErr != nil {
				err = errors.Append(err, unlockErr)
			}

			// No-op if called more than once
			locked = false
		}

		return err
	}

	return locked, close, nil
}

func (s *Store) lockKey() int32 {
	return locker.StringKey(fmt.Sprintf("%s:migrations", s.schemaName))
}

// Up runs the given definition's up query.
func (s *Store) Up(ctx context.Context, definition definition.Definition) (err error) {
	ctx, _, endObservation := s.operations.up.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return s.Exec(ctx, definition.UpQuery)
}

// Down runs the given definition's down query.
func (s *Store) Down(ctx context.Context, definition definition.Definition) (err error) {
	ctx, _, endObservation := s.operations.down.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return s.Exec(ctx, definition.DownQuery)
}

// IndexStatus returns an object describing the current validity status and creation progress of the
// index with the given name. If the index does not exist, a false-valued flag is returned.
func (s *Store) IndexStatus(ctx context.Context, tableName, indexName string) (_ shared.IndexStatus, _ bool, err error) {
	ctx, _, endObservation := s.operations.indexStatus.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return scanFirstIndexStatus(s.Query(ctx, sqlf.Sprintf(indexStatusQuery, tableName, indexName)))
}

const indexStatusQuery = `
SELECT
	pi.indisvalid,
	pi.indisready,
	pi.indislive,
	p.phase,
	p.lockers_total,
	p.lockers_done,
	p.blocks_total,
	p.blocks_done,
	p.tuples_total,
	p.tuples_done
FROM pg_catalog.pg_stat_all_indexes ai
JOIN pg_catalog.pg_index pi ON pi.indexrelid = ai.indexrelid
LEFT JOIN pg_catalog.pg_stat_progress_create_index p ON p.relid = ai.relid AND p.index_relid = ai.indexrelid
WHERE
	ai.relname = %s AND
	ai.indexrelname = %s
`

// WithMigrationLog runs the given function while writing its progress to a migration log associated
// with the given definition. All users are assumed to run either `s.Up` or `s.Down` as part of the
// given function, among any other behaviors that are necessary to perform in the _critical section_.
func (s *Store) WithMigrationLog(ctx context.Context, definition definition.Definition, up bool, f func() error) (err error) {
	ctx, _, endObservation := s.operations.withMigrationLog.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	logID, err := s.createMigrationLog(ctx, definition.ID, up)
	if err != nil {
		return err
	}

	defer func() {
		if execErr := s.Exec(ctx, sqlf.Sprintf(
			`UPDATE migration_logs SET finished_at = NOW(), success = %s, error_message = %s WHERE id = %d`,
			err == nil,
			errMsgPtr(err),
			logID,
		)); execErr != nil {
			err = errors.Append(err, execErr)
		}
	}()

	if err := f(); err != nil {
		return err
	}

	return nil
}

func (s *Store) createMigrationLog(ctx context.Context, definitionVersion int, up bool) (_ int, err error) {
	tx, err := s.Transact(ctx)
	if err != nil {
		return 0, err
	}
	defer func() { err = tx.Done(err) }()

	id, _, err := basestore.ScanFirstInt(tx.Query(ctx, sqlf.Sprintf(
		`
			INSERT INTO migration_logs (
				migration_logs_schema_version,
				schema,
				version,
				up,
				started_at
			) VALUES (%s, %s, %s, %s, NOW())
			RETURNING id
		`,
		currentMigrationLogSchemaVersion,
		s.schemaName,
		definitionVersion,
		up,
	)))
	if err != nil {
		return 0, err
	}

	return id, nil
}

func errMsgPtr(err error) *string {
	if err == nil {
		return nil
	}

	text := err.Error()
	return &text
}

type migrationLog struct {
	Schema  string
	Version int
	Up      bool
	Success *bool
}

// scanMigrationLogs scans a slice of migration logs from the return value of `*Store.query`.
func scanMigrationLogs(rows *sql.Rows, queryErr error) (_ []migrationLog, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	var logs []migrationLog
	for rows.Next() {
		var log migrationLog

		if err := rows.Scan(
			&log.Schema,
			&log.Version,
			&log.Up,
			&log.Success,
		); err != nil {
			return nil, err
		}

		logs = append(logs, log)
	}

	return logs, nil
}

// scanFirstIndexStatus scans a slice of index status objects from the return value of `*Store.query`.
func scanFirstIndexStatus(rows *sql.Rows, queryErr error) (status shared.IndexStatus, _ bool, err error) {
	if queryErr != nil {
		return shared.IndexStatus{}, false, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	if rows.Next() {
		if err := rows.Scan(
			&status.IsValid,
			&status.IsReady,
			&status.IsLive,
			&status.Phase,
			&status.LockersDone,
			&status.LockersTotal,
			&status.BlocksDone,
			&status.BlocksTotal,
			&status.TuplesDone,
			&status.TuplesTotal,
		); err != nil {
			return shared.IndexStatus{}, false, err
		}

		return status, true, nil
	}

	return shared.IndexStatus{}, false, nil
}

// humanizeSchemaName converts the golang-migrate/migration_logs.schema name into the name
// defined by the definitions in the migrations/ directory. Hopefully we cna get rid of this
// difference in the future, but that requires a bit of migratory work.
func humanizeSchemaName(schemaName string) string {
	if schemaName == "schema_migrations" {
		return "frontend"
	}

	return strings.TrimSuffix(schemaName, "_schema_migrations")
}

var quote = sqlf.Sprintf

// isMissingRelation returns true if the given error occurs due to a missing relation in Postgres.
func isMissingRelation(err error) bool {
	var pgErr *pgconn.PgError
	if !errors.As(err, &pgErr) {
		return false
	}

	return pgErr.Code == "42P01"
}
