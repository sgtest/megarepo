package store

import (
	"context"
	"database/sql"
	"time"

	"github.com/hashicorp/go-multierror"
	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
)

// Store is the interface to Postgres for precise-code-intel features.
type Store interface {
	// Transact returns a store whose methods operate within the context of a transaction.
	// This method will return an error if the underlying store cannot be interface upgraded
	// to a TxBeginner.
	Transact(ctx context.Context) (Store, error)

	// Savepoint creates a named position in the transaction from which all additional work
	// can be discarded. The returned identifier can be passed to RollbackToSavepont to undo
	// all the work since this call.
	Savepoint(ctx context.Context) (string, error)

	// RollbackToSavepoint throws away all the work on the underlying transaction since the
	// savepoint with the given name was created.
	RollbackToSavepoint(ctx context.Context, savepointID string) error

	// Done commits underlying the transaction on a nil error value and performs a rollback
	// otherwise. If an error occurs during commit or rollback of the transaction, the error
	// is added to the resulting error value. If the store does not wrap a transaction the
	// original error value is returned unchanged.
	Done(err error) error

	// GetUploadByID returns an upload by its identifier and boolean flag indicating its existence.
	GetUploadByID(ctx context.Context, id int) (Upload, bool, error)

	// GetUploads returns a list of uploads and the total count of records matching the given conditions.
	GetUploads(ctx context.Context, opts GetUploadsOptions) ([]Upload, int, error)

	// QueueSize returns the number of uploads in the queued state.
	QueueSize(ctx context.Context) (int, error)

	// InsertUpload inserts a new upload and returns its identifier.
	InsertUpload(ctx context.Context, upload Upload) (int, error)

	// AddUploadPart adds the part index to the given upload's uploaded parts array. This method is idempotent
	// (the resulting array is deduplicated on update).
	AddUploadPart(ctx context.Context, uploadID, partIndex int) error

	// MarkQueued updates the state of the upload to queued.
	MarkQueued(ctx context.Context, uploadID int) error

	// MarkComplete updates the state of the upload to complete.
	MarkComplete(ctx context.Context, id int) error

	// MarkErrored updates the state of the upload to errored and updates the failure summary data.
	MarkErrored(ctx context.Context, id int, failureMessage string) error

	// Dequeue selects the oldest queued upload and locks it with a transaction. If there is such an upload, the
	// upload is returned along with a store instance which wraps the transaction. This transaction must be closed.
	// If there is no such unlocked upload, a zero-value upload and nil store will be returned along with a false
	// valued flag. This method must not be called from within a transaction.
	Dequeue(ctx context.Context) (Upload, Store, bool, error)

	// Requeue updates the state of the upload to queued and adds a processing delay before the next dequeue attempt.
	Requeue(ctx context.Context, id int, after time.Time) error

	// GetStates returns the states for the uploads with the given identifiers.
	GetStates(ctx context.Context, ids []int) (map[int]string, error)

	// DeleteUploadByID deletes an upload by its identifier. If the upload was visible at the tip of its repository's default branch,
	// the visibility of all uploads for that repository are recalculated. The getTipCommit function is expected to return the newest
	// commit on the default branch when invoked.
	DeleteUploadByID(ctx context.Context, id int, getTipCommit GetTipCommitFunc) (bool, error)

	// DeleteUploadsWithoutRepository deletes uploads associated with repositories that were deleted at least
	// DeletedRepositoryGracePeriod ago. This returns the repository identifier mapped to the number of uploads
	// that were removed for that repository.
	DeleteUploadsWithoutRepository(ctx context.Context, now time.Time) (map[int]int, error)

	// ResetStalled moves all unlocked uploads processing for more than `StalledUploadMaxAge` back to the queued state.
	// In order to prevent input that continually crashes worker instances, uploads that have been reset more than
	// UploadMaxNumResets times will be marked as errored. This method returns a list of updated and errored upload
	// identifiers.
	ResetStalled(ctx context.Context, now time.Time) ([]int, []int, error)

	// GetDumpByID returns a dump by its identifier and boolean flag indicating its existence.
	GetDumpByID(ctx context.Context, id int) (Dump, bool, error)

	// FindClosestDumps returns the set of dumps that can most accurately answer queries for the given repository, commit, path, and
	// optional indexer. If rootMustEnclosePath is true, then only dumps with a root which is a prefix of path are returned. Otherwise,
	// any dump with a root intersecting the given path is returned.
	FindClosestDumps(ctx context.Context, repositoryID int, commit, path string, rootMustEnclosePath bool, indexer string) ([]Dump, error)

	// DeleteOldestDump deletes the oldest dump that is not currently visible at the tip of its repository's default branch.
	// This method returns the deleted dump's identifier and a flag indicating its (previous) existence.
	DeleteOldestDump(ctx context.Context) (int, bool, error)

	// UpdateDumpsVisibleFromTip recalculates the visible_at_tip flag of all dumps of the given repository.
	UpdateDumpsVisibleFromTip(ctx context.Context, repositoryID int, tipCommit string) error

	// DeleteOverlapapingDumps deletes all completed uploads for the given repository with the same
	// commit, root, and indexer. This is necessary to perform during conversions before changing
	// the state of a processing upload to completed as there is a unique index on these four columns.
	DeleteOverlappingDumps(ctx context.Context, repositoryID int, commit, root, indexer string) error

	// GetPackage returns the dump that provides the package with the given scheme, name, and version and a flag indicating its existence.
	GetPackage(ctx context.Context, scheme, name, version string) (Dump, bool, error)

	// UpdatePackages bulk upserts package data.
	UpdatePackages(ctx context.Context, packages []types.Package) error

	// SameRepoPager returns a ReferencePager for dumps that belong to the given repository and commit and reference the package with the
	// given scheme, name, and version.
	SameRepoPager(ctx context.Context, repositoryID int, commit, scheme, name, version string, limit int) (int, ReferencePager, error)

	// UpdatePackageReferences bulk inserts package reference data.
	UpdatePackageReferences(ctx context.Context, packageReferences []types.PackageReference) error

	// PackageReferencePager returns a ReferencePager for dumps that belong to a remote repository (distinct from the given repository id)
	// and reference the package with the given scheme, name, and version. All resulting dumps are visible at the tip of their repository's
	// default branch.
	PackageReferencePager(ctx context.Context, scheme, name, version string, repositoryID, limit int) (int, ReferencePager, error)

	// HasCommit determines if the given commit is known for the given repository.
	HasCommit(ctx context.Context, repositoryID int, commit string) (bool, error)

	// UpdateCommits upserts commits/parent-commit relations for the given repository ID.
	UpdateCommits(ctx context.Context, repositoryID int, commits map[string][]string) error

	// IndexableRepositories returns the identifiers of all indexable repositories.
	IndexableRepositories(ctx context.Context, opts IndexableRepositoryQueryOptions) ([]IndexableRepository, error)

	// UpdateIndexableRepository updates the metadata for an indexable repository. If the repository is not
	// already marked as indexable, a new record will be created.
	UpdateIndexableRepository(ctx context.Context, indexableRepository UpdateableIndexableRepository, now time.Time) error

	// ResetIndexableRepositories zeroes the event counts for indexable repositories that have not been updated
	// since lastUpdatedBefore.
	ResetIndexableRepositories(ctx context.Context, lastUpdatedBefore time.Time) error

	// GetIndexByID returns an index by its identifier and boolean flag indicating its existence.
	GetIndexByID(ctx context.Context, id int) (Index, bool, error)

	// GetIndexes returns a list of indexes and the total count of records matching the given conditions.
	GetIndexes(ctx context.Context, opts GetIndexesOptions) ([]Index, int, error)

	// IndexQueueSize returns the number of indexes in the queued state.
	IndexQueueSize(ctx context.Context) (int, error)

	// IsQueued returns true if there is an index or an upload for the repository and commit.
	IsQueued(ctx context.Context, repositoryID int, commit string) (bool, error)

	// InsertIndex inserts a new index and returns its identifier.
	InsertIndex(ctx context.Context, index Index) (int, error)

	// MarkIndexComplete updates the state of the index to complete.
	MarkIndexComplete(ctx context.Context, id int) (err error)

	// MarkIndexErrored updates the state of the index to errored and updates the failure summary data.
	MarkIndexErrored(ctx context.Context, id int, failureMessage string) (err error)

	// DequeueIndex selects the oldest queued index and locks it with a transaction. If there is such an index,
	// the index is returned along with a store instance which wraps the transaction. This transaction must be
	// closed. If there is no such unlocked index, a zero-value index and nil store will be returned along with
	// a false valued flag. This method must not be called from within a transaction.
	DequeueIndex(ctx context.Context) (Index, Store, bool, error)

	// RequeueIndex updates the state of the index to queued and adds a processing delay before the next dequeue attempt.
	RequeueIndex(ctx context.Context, id int, after time.Time) error

	// DeleteIndexByID deletes an index by its identifier.
	DeleteIndexByID(ctx context.Context, id int) (bool, error)

	// DeleteIndexesWithoutRepository deletes indexes associated with repositories that were deleted at least
	// DeletedRepositoryGracePeriod ago. This returns the repository identifier mapped to the number of indexes
	// that were removed for that repository.
	DeleteIndexesWithoutRepository(ctx context.Context, now time.Time) (map[int]int, error)

	// ResetStalledIndexes moves all unlocked index processing for more than `StalledIndexMaxAge` back to the
	// queued state. In order to prevent input that continually crashes indexer instances, indexes that have
	// been reset more than IndexMaxNumResets times will be marked as errored. This method returns a list of
	// updated and errored index identifiers.
	ResetStalledIndexes(ctx context.Context, now time.Time) ([]int, []int, error)

	// RepoUsageStatistics reads recent event log records and returns the number of search-based and precise
	// code intelligence activity within the last week grouped by repository. The resulting slice is ordered
	// by search then precise event counts.
	RepoUsageStatistics(ctx context.Context) ([]RepoUsageStatistics, error)

	// RepoName returns the name for the repo with the given identifier.
	RepoName(ctx context.Context, repositoryID int) (string, error)
}

// GetTipCommitFunc returns the head commit for the given repository.
type GetTipCommitFunc func(ctx context.Context, repositoryID int) (string, error)

type store struct {
	db           dbutil.DB
	savepointIDs []string
}

var _ Store = &store{}

// New creates a new instance of store connected to the given Postgres DSN.
func New(postgresDSN string) (Store, error) {
	db, err := dbutil.NewDB(postgresDSN, "codeintel")
	if err != nil {
		return nil, err
	}

	return &store{db: db}, nil
}

func NewWithHandle(db *sql.DB) Store {
	return &store{db: db}
}

// query performs QueryContext on the underlying connection.
func (s *store) query(ctx context.Context, query *sqlf.Query) (*sql.Rows, error) {
	return s.db.QueryContext(ctx, query.Query(sqlf.PostgresBindVar), query.Args()...)
}

// queryForEffect performs a query and throws away the result.
func (s *store) queryForEffect(ctx context.Context, query *sqlf.Query) error {
	rows, err := s.query(ctx, query)
	if err != nil {
		return err
	}
	return closeRows(rows, nil)
}

// scanStrings scans a slice of strings from the return value of `*store.query`.
func scanStrings(rows *sql.Rows, queryErr error) (_ []string, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	var values []string
	for rows.Next() {
		var value string
		if err := rows.Scan(&value); err != nil {
			return nil, err
		}

		values = append(values, value)
	}

	return values, nil
}

// scanFirstString scans a slice of strings from the return value of `*store.query` and returns the first.
func scanFirstString(rows *sql.Rows, err error) (string, bool, error) {
	values, err := scanStrings(rows, err)
	if err != nil || len(values) == 0 {
		return "", false, err
	}
	return values[0], true, nil
}

// scanInts scans a slice of ints from the return value of `*store.query`.
func scanInts(rows *sql.Rows, queryErr error) (_ []int, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = closeRows(rows, err) }()

	var values []int
	for rows.Next() {
		var value int
		if err := rows.Scan(&value); err != nil {
			return nil, err
		}

		values = append(values, value)
	}

	return values, nil
}

// scanFirstInt scans a slice of ints from the return value of `*store.query` and returns the first.
func scanFirstInt(rows *sql.Rows, err error) (int, bool, error) {
	values, err := scanInts(rows, err)
	if err != nil || len(values) == 0 {
		return 0, false, err
	}
	return values[0], true, nil
}

// closeRows closes the rows object and checks its error value.
func closeRows(rows *sql.Rows, err error) error {
	if closeErr := rows.Close(); closeErr != nil {
		err = multierror.Append(err, closeErr)
	}

	if rowsErr := rows.Err(); rowsErr != nil {
		err = multierror.Append(err, rowsErr)
	}

	return err
}

// intsToQueries converts a slice of ints into a slice of queries.
func intsToQueries(values []int) []*sqlf.Query {
	var queries []*sqlf.Query
	for _, value := range values {
		queries = append(queries, sqlf.Sprintf("%d", value))
	}

	return queries
}
