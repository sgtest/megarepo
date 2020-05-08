package db

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// An ObservedDB wraps another DB with error logging, Prometheus metrics, and tracing.
type ObservedDB struct {
	db                                 DB
	getUploadByIDOperation             *observation.Operation
	getUploadsByRepoOperation          *observation.Operation
	enqueueOperation                   *observation.Operation
	dequeueOperation                   *observation.Operation
	getStatesOperation                 *observation.Operation
	deleteUploadByIDOperation          *observation.Operation
	resetStalledOperation              *observation.Operation
	getDumpByIDOperation               *observation.Operation
	findClosestDumpsOperation          *observation.Operation
	deleteOldestDumpOperation          *observation.Operation
	updateDumpsVisibleFromTipOperation *observation.Operation
	deleteOverlappingDumpsOperation    *observation.Operation
	getPackageOperation                *observation.Operation
	updatePackagesOperation            *observation.Operation
	sameRepoPagerOperation             *observation.Operation
	updatePackageReferencesOperation   *observation.Operation
	packageReferencePagerOperation     *observation.Operation
	hasCommitOperation                 *observation.Operation
	updateCommitsOperation             *observation.Operation
	repoNameOperation                  *observation.Operation
}

var _ DB = &ObservedDB{}

// NewObservedDB wraps the given DB with error logging, Prometheus metrics, and tracing.
func NewObserved(db DB, observationContext *observation.Context, subsystem string) DB {
	metrics := metrics.NewOperationMetrics(
		observationContext.Registerer,
		subsystem,
		"db",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of results returned"),
	)

	return &ObservedDB{
		db: db,
		getUploadByIDOperation: observationContext.Operation(observation.Op{
			Name:         "DB.GetUploadByID",
			MetricLabels: []string{"get_upload_by_id"},
			Metrics:      metrics,
		}),
		getUploadsByRepoOperation: observationContext.Operation(observation.Op{
			Name:         "DB.GetUploadsByRepo",
			MetricLabels: []string{"get_uploads_by_repo"},
			Metrics:      metrics,
		}),
		enqueueOperation: observationContext.Operation(observation.Op{
			Name:         "DB.Enqueue",
			MetricLabels: []string{"enqueue"},
			Metrics:      metrics,
		}),
		dequeueOperation: observationContext.Operation(observation.Op{
			Name:         "DB.Dequeue",
			MetricLabels: []string{"dequeue"},
			Metrics:      metrics,
		}),
		getStatesOperation: observationContext.Operation(observation.Op{
			Name:         "DB.GetStates",
			MetricLabels: []string{"get_states"},
			Metrics:      metrics,
		}),
		deleteUploadByIDOperation: observationContext.Operation(observation.Op{
			Name:         "DB.DeleteUploadByID",
			MetricLabels: []string{"delete_upload_by_id"},
			Metrics:      metrics,
		}),
		resetStalledOperation: observationContext.Operation(observation.Op{
			Name:         "DB.ResetStalled",
			MetricLabels: []string{"reset_stalled"},
			Metrics:      metrics,
		}),
		getDumpByIDOperation: observationContext.Operation(observation.Op{
			Name:         "DB.GetDumpByID",
			MetricLabels: []string{"get_dump_by_id"},
			Metrics:      metrics,
		}),
		findClosestDumpsOperation: observationContext.Operation(observation.Op{
			Name:         "DB.FindClosestDumps",
			MetricLabels: []string{"find_closest_dumps"},
			Metrics:      metrics,
		}),
		deleteOldestDumpOperation: observationContext.Operation(observation.Op{
			Name:         "DB.DeleteOldestDump",
			MetricLabels: []string{"delete_oldest_dump"},
			Metrics:      metrics,
		}),
		updateDumpsVisibleFromTipOperation: observationContext.Operation(observation.Op{
			Name:         "DB.UpdateDumpsVisibleFromTip",
			MetricLabels: []string{"update_dumps_visible_from_tip"},
			Metrics:      metrics,
		}),
		deleteOverlappingDumpsOperation: observationContext.Operation(observation.Op{
			Name:         "DB.DeleteOverlappingDumps",
			MetricLabels: []string{"delete_overlapping_dumps"},
			Metrics:      metrics,
		}),
		getPackageOperation: observationContext.Operation(observation.Op{
			Name:         "DB.GetPackage",
			MetricLabels: []string{"get_package"},
			Metrics:      metrics,
		}),
		updatePackagesOperation: observationContext.Operation(observation.Op{
			Name:         "DB.UpdatePackages",
			MetricLabels: []string{"update_packages"},
			Metrics:      metrics,
		}),
		sameRepoPagerOperation: observationContext.Operation(observation.Op{
			Name:         "DB.SameRepoPager",
			MetricLabels: []string{"same_repo_pager"},
			Metrics:      metrics,
		}),
		updatePackageReferencesOperation: observationContext.Operation(observation.Op{
			Name:         "DB.UpdatePackageReferences",
			MetricLabels: []string{"update_package_references"},
			Metrics:      metrics,
		}),
		packageReferencePagerOperation: observationContext.Operation(observation.Op{
			Name:         "DB.PackageReferencePager",
			MetricLabels: []string{"package_reference_pager"},
			Metrics:      metrics,
		}),
		hasCommitOperation: observationContext.Operation(observation.Op{
			Name:         "DB.HasCommit",
			MetricLabels: []string{"has_commit"},
			Metrics:      metrics,
		}),
		updateCommitsOperation: observationContext.Operation(observation.Op{
			Name:         "DB.UpdateCommits",
			MetricLabels: []string{"update_commits"},
			Metrics:      metrics,
		}),
		repoNameOperation: observationContext.Operation(observation.Op{
			Name:         "DB.RepoName",
			MetricLabels: []string{"repo_name"},
			Metrics:      metrics,
		}),
	}
}

// Transact calls into the inner DB.
func (db *ObservedDB) Transact(ctx context.Context) (DB, error) {
	return db.db.Transact(ctx)
}

// Done calls into the inner DB.
func (db *ObservedDB) Done(err error) error {
	return db.db.Done(err)
}

// GetUploadByID calls into the inner DB and registers the observed results.
func (db *ObservedDB) GetUploadByID(ctx context.Context, id int) (_ Upload, _ bool, err error) {
	ctx, endObservation := db.getUploadByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.GetUploadByID(ctx, id)
}

// GetUploadsByRepo calls into the inner DB and registers the observed results.
func (db *ObservedDB) GetUploadsByRepo(ctx context.Context, repositoryID int, state, term string, visibleAtTip bool, limit, offset int) (uploads []Upload, _ int, err error) {
	ctx, endObservation := db.getUploadsByRepoOperation.With(ctx, &err, observation.Args{})
	defer func() {
		endObservation(float64(len(uploads)), observation.Args{})
	}()

	return db.db.GetUploadsByRepo(ctx, repositoryID, state, term, visibleAtTip, limit, offset)
}

// Enqueue calls into the inner DB and registers the observed results.
func (db *ObservedDB) Enqueue(ctx context.Context, commit, root string, repositoryID int, indexerName string) (_ int, err error) {
	ctx, endObservation := db.enqueueOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.Enqueue(ctx, commit, root, repositoryID, indexerName)
}

// Dequeue calls into the inner DB and registers the observed results.
func (db *ObservedDB) Dequeue(ctx context.Context) (_ Upload, _ JobHandle, _ bool, err error) {
	ctx, endObservation := db.dequeueOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.Dequeue(ctx)
}

// GetStates calls into the inner DB and registers the observed results.
func (db *ObservedDB) GetStates(ctx context.Context, ids []int) (states map[int]string, err error) {
	ctx, endObservation := db.getStatesOperation.With(ctx, &err, observation.Args{})
	defer func() {
		endObservation(float64(len(states)), observation.Args{})
	}()

	return db.db.GetStates(ctx, ids)
}

// DeleteUploadByID calls into the inner DB and registers the observed results.
func (db *ObservedDB) DeleteUploadByID(ctx context.Context, id int, getTipCommit GetTipCommitFn) (_ bool, err error) {
	ctx, endObservation := db.deleteUploadByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.DeleteUploadByID(ctx, id, getTipCommit)
}

// ResetStalled calls into the inner DB and registers the observed results.
func (db *ObservedDB) ResetStalled(ctx context.Context, now time.Time) (ids []int, err error) {
	ctx, endObservation := db.resetStalledOperation.With(ctx, &err, observation.Args{})
	defer func() {
		endObservation(float64(len(ids)), observation.Args{})
	}()

	return db.db.ResetStalled(ctx, now)
}

// GetDumpByID calls into the inner DB and registers the observed results.
func (db *ObservedDB) GetDumpByID(ctx context.Context, id int) (_ Dump, _ bool, err error) {
	ctx, endObservation := db.getDumpByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.GetDumpByID(ctx, id)
}

// FindClosestDumps calls into the inner DB and registers the observed results.
func (db *ObservedDB) FindClosestDumps(ctx context.Context, repositoryID int, commit, file string) (dumps []Dump, err error) {
	ctx, endObservation := db.findClosestDumpsOperation.With(ctx, &err, observation.Args{})
	defer func() {
		endObservation(float64(len(dumps)), observation.Args{})
	}()

	return db.db.FindClosestDumps(ctx, repositoryID, commit, file)
}

// DeleteOldestDump calls into the inner DB and registers the observed results.
func (db *ObservedDB) DeleteOldestDump(ctx context.Context) (_ int, _ bool, err error) {
	ctx, endObservation := db.deleteOldestDumpOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.DeleteOldestDump(ctx)
}

// UpdateDumpsVisibleFromTip calls into the inner DB and registers the observed results.
func (db *ObservedDB) UpdateDumpsVisibleFromTip(ctx context.Context, repositoryID int, tipCommit string) (err error) {
	ctx, endObservation := db.updateDumpsVisibleFromTipOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.UpdateDumpsVisibleFromTip(ctx, repositoryID, tipCommit)
}

// DeleteOverlappingDumps calls into the inner DB and registers the observed results.
func (db *ObservedDB) DeleteOverlappingDumps(ctx context.Context, repositoryID int, commit, root, indexer string) (err error) {
	ctx, endObservation := db.deleteOverlappingDumpsOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.DeleteOverlappingDumps(ctx, repositoryID, commit, root, indexer)
}

// GetPackage calls into the inner DB and registers the observed results.
func (db *ObservedDB) GetPackage(ctx context.Context, scheme, name, version string) (_ Dump, _ bool, err error) {
	ctx, endObservation := db.getPackageOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.GetPackage(ctx, scheme, name, version)
}

// UpdatePackages calls into the inner DB and registers the observed results.
func (db *ObservedDB) UpdatePackages(ctx context.Context, packages []types.Package) (err error) {
	ctx, endObservation := db.updatePackagesOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.UpdatePackages(ctx, packages)
}

// SameRepoPager calls into the inner DB and registers the observed results.
func (db *ObservedDB) SameRepoPager(ctx context.Context, repositoryID int, commit, scheme, name, version string, limit int) (_ int, _ ReferencePager, err error) {
	ctx, endObservation := db.sameRepoPagerOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.SameRepoPager(ctx, repositoryID, commit, scheme, name, version, limit)
}

// UpdatePackageReferences calls into the inner DB and registers the observed results.
func (db *ObservedDB) UpdatePackageReferences(ctx context.Context, packageReferences []types.PackageReference) (err error) {
	ctx, endObservation := db.updatePackageReferencesOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.UpdatePackageReferences(ctx, packageReferences)
}

// PackageReferencePager calls into the inner DB and registers the observed results.
func (db *ObservedDB) PackageReferencePager(ctx context.Context, scheme, name, version string, repositoryID, limit int) (_ int, _ ReferencePager, err error) {
	ctx, endObservation := db.packageReferencePagerOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.PackageReferencePager(ctx, scheme, name, version, repositoryID, limit)
}

// HasCommit calls into the inner DB and registers the observed results.
func (db *ObservedDB) HasCommit(ctx context.Context, repositoryID int, commit string) (_ bool, err error) {
	ctx, endObservation := db.hasCommitOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.HasCommit(ctx, repositoryID, commit)
}

// UpdateCommits calls into the inner DB and registers the observed results.
func (db *ObservedDB) UpdateCommits(ctx context.Context, repositoryID int, commits map[string][]string) (err error) {
	ctx, endObservation := db.updateCommitsOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.UpdateCommits(ctx, repositoryID, commits)
}

// RepoName calls into the inner DB and registers the observed results.
func (db *ObservedDB) RepoName(ctx context.Context, repositoryID int) (_ string, err error) {
	ctx, endObservation := db.repoNameOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return db.db.RepoName(ctx, repositoryID)
}
