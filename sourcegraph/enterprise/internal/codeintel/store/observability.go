package store

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// An ObservedStore wraps another store with error logging, Prometheus metrics, and tracing.
type ObservedStore struct {
	store                              Store
	savepointOperation                 *observation.Operation
	rollbackToSavepointOperation       *observation.Operation
	doneOperation                      *observation.Operation
	getUploadByIDOperation             *observation.Operation
	getUploadsOperation                *observation.Operation
	queueSizeOperation                 *observation.Operation
	insertUploadOperation              *observation.Operation
	addUploadPartOperation             *observation.Operation
	markQueuedOperation                *observation.Operation
	markCompleteOperation              *observation.Operation
	markErroredOperation               *observation.Operation
	dequeueOperation                   *observation.Operation
	requeueOperation                   *observation.Operation
	getStatesOperation                 *observation.Operation
	deleteUploadByIDOperation          *observation.Operation
	resetStalledOperation              *observation.Operation
	getDumpIDsOperation                *observation.Operation
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
	indexableRepositoriesOperation     *observation.Operation
	updateIndexableRepositoryOperation *observation.Operation
	getIndexByIDOperation              *observation.Operation
	getIndexesOperation                *observation.Operation
	indexQueueSizeOperation            *observation.Operation
	isQueuedOperation                  *observation.Operation
	insertIndexOperation               *observation.Operation
	markIndexCompleteOperation         *observation.Operation
	markIndexErroredOperation          *observation.Operation
	dequeueIndexOperation              *observation.Operation
	requeueIndexOperation              *observation.Operation
	deleteIndexByIdOperation           *observation.Operation
	resetStalledIndexesOperation       *observation.Operation
	repoUsageStatisticsOperation       *observation.Operation
	repoNameOperation                  *observation.Operation
}

var _ Store = &ObservedStore{}

// NewObserved wraps the given store with error logging, Prometheus metrics, and tracing.
func NewObserved(store Store, observationContext *observation.Context) Store {
	metrics := metrics.NewOperationMetrics(
		observationContext.Registerer,
		"code_intel_store",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of results returned"),
	)

	return &ObservedStore{
		store: store,
		savepointOperation: observationContext.Operation(observation.Op{
			Name:         "store.Savepoint",
			MetricLabels: []string{"savepoint"},
			Metrics:      metrics,
		}),
		rollbackToSavepointOperation: observationContext.Operation(observation.Op{
			Name:         "store.RollbackToSavepoint",
			MetricLabels: []string{"rollback_to_savepoint"},
			Metrics:      metrics,
		}),
		doneOperation: observationContext.Operation(observation.Op{
			Name:         "store.Done",
			MetricLabels: []string{"done"},
			Metrics:      metrics,
		}),
		getUploadByIDOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetUploadByID",
			MetricLabels: []string{"get_upload_by_id"},
			Metrics:      metrics,
		}),
		getUploadsOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetUploads",
			MetricLabels: []string{"get_uploads"},
			Metrics:      metrics,
		}),
		queueSizeOperation: observationContext.Operation(observation.Op{
			Name:         "store.QueueSize",
			MetricLabels: []string{"queue_size"},
			Metrics:      metrics,
		}),
		insertUploadOperation: observationContext.Operation(observation.Op{
			Name:         "store.InsertUpload",
			MetricLabels: []string{"insert_upload"},
			Metrics:      metrics,
		}),
		addUploadPartOperation: observationContext.Operation(observation.Op{
			Name:         "store.AddUploadPart",
			MetricLabels: []string{"add_upload_part"},
			Metrics:      metrics,
		}),
		markQueuedOperation: observationContext.Operation(observation.Op{
			Name:         "store.MarkQueued",
			MetricLabels: []string{"mark_queued"},
			Metrics:      metrics,
		}),
		markCompleteOperation: observationContext.Operation(observation.Op{
			Name:         "store.MarkComplete",
			MetricLabels: []string{"mark_complete"},
			Metrics:      metrics,
		}),
		markErroredOperation: observationContext.Operation(observation.Op{
			Name:         "store.MarkErrored",
			MetricLabels: []string{"mark_errored"},
			Metrics:      metrics,
		}),
		dequeueOperation: observationContext.Operation(observation.Op{
			Name:         "store.Dequeue",
			MetricLabels: []string{"dequeue"},
			Metrics:      metrics,
		}),
		requeueOperation: observationContext.Operation(observation.Op{
			Name:         "store.Requeue",
			MetricLabels: []string{"requeue"},
			Metrics:      metrics,
		}),
		getStatesOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetStates",
			MetricLabels: []string{"get_states"},
			Metrics:      metrics,
		}),
		deleteUploadByIDOperation: observationContext.Operation(observation.Op{
			Name:         "store.DeleteUploadByID",
			MetricLabels: []string{"delete_upload_by_id"},
			Metrics:      metrics,
		}),
		resetStalledOperation: observationContext.Operation(observation.Op{
			Name:         "store.ResetStalled",
			MetricLabels: []string{"reset_stalled"},
			Metrics:      metrics,
		}),
		getDumpIDsOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetDumpIDs",
			MetricLabels: []string{"get_dump_ids"},
			Metrics:      metrics,
		}),
		getDumpByIDOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetDumpByID",
			MetricLabels: []string{"get_dump_by_id"},
			Metrics:      metrics,
		}),
		findClosestDumpsOperation: observationContext.Operation(observation.Op{
			Name:         "store.FindClosestDumps",
			MetricLabels: []string{"find_closest_dumps"},
			Metrics:      metrics,
		}),
		deleteOldestDumpOperation: observationContext.Operation(observation.Op{
			Name:         "store.DeleteOldestDump",
			MetricLabels: []string{"delete_oldest_dump"},
			Metrics:      metrics,
		}),
		updateDumpsVisibleFromTipOperation: observationContext.Operation(observation.Op{
			Name:         "store.UpdateDumpsVisibleFromTip",
			MetricLabels: []string{"update_dumps_visible_from_tip"},
			Metrics:      metrics,
		}),
		deleteOverlappingDumpsOperation: observationContext.Operation(observation.Op{
			Name:         "store.DeleteOverlappingDumps",
			MetricLabels: []string{"delete_overlapping_dumps"},
			Metrics:      metrics,
		}),
		getPackageOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetPackage",
			MetricLabels: []string{"get_package"},
			Metrics:      metrics,
		}),
		updatePackagesOperation: observationContext.Operation(observation.Op{
			Name:         "store.UpdatePackages",
			MetricLabels: []string{"update_packages"},
			Metrics:      metrics,
		}),
		sameRepoPagerOperation: observationContext.Operation(observation.Op{
			Name:         "store.SameRepoPager",
			MetricLabels: []string{"same_repo_pager"},
			Metrics:      metrics,
		}),
		updatePackageReferencesOperation: observationContext.Operation(observation.Op{
			Name:         "store.UpdatePackageReferences",
			MetricLabels: []string{"update_package_references"},
			Metrics:      metrics,
		}),
		packageReferencePagerOperation: observationContext.Operation(observation.Op{
			Name:         "store.PackageReferencePager",
			MetricLabels: []string{"package_reference_pager"},
			Metrics:      metrics,
		}),
		hasCommitOperation: observationContext.Operation(observation.Op{
			Name:         "store.HasCommit",
			MetricLabels: []string{"has_commit"},
			Metrics:      metrics,
		}),
		updateCommitsOperation: observationContext.Operation(observation.Op{
			Name:         "store.UpdateCommits",
			MetricLabels: []string{"update_commits"},
			Metrics:      metrics,
		}),
		indexableRepositoriesOperation: observationContext.Operation(observation.Op{
			Name:         "store.IndexableRepositories",
			MetricLabels: []string{"indexable_repositories"},
			Metrics:      metrics,
		}),
		updateIndexableRepositoryOperation: observationContext.Operation(observation.Op{
			Name:         "store.UpdateIndexableRepository",
			MetricLabels: []string{"update_indexable_repository"},
			Metrics:      metrics,
		}),
		getIndexByIDOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetIndexByID",
			MetricLabels: []string{"get_index_by_id"},
			Metrics:      metrics,
		}),
		getIndexesOperation: observationContext.Operation(observation.Op{
			Name:         "store.GetIndexes",
			MetricLabels: []string{"get_indexes"},
			Metrics:      metrics,
		}),
		indexQueueSizeOperation: observationContext.Operation(observation.Op{
			Name:         "store.IndexQueueSize",
			MetricLabels: []string{"index_queue_size"},
			Metrics:      metrics,
		}),
		isQueuedOperation: observationContext.Operation(observation.Op{
			Name:         "store.IsQueued",
			MetricLabels: []string{"is_queued"},
			Metrics:      metrics,
		}),
		insertIndexOperation: observationContext.Operation(observation.Op{
			Name:         "store.InsertIndex",
			MetricLabels: []string{"insert_index"},
			Metrics:      metrics,
		}),
		markIndexCompleteOperation: observationContext.Operation(observation.Op{
			Name:         "store.MarkIndexComplete",
			MetricLabels: []string{"mark_index_complete"},
			Metrics:      metrics,
		}),
		markIndexErroredOperation: observationContext.Operation(observation.Op{
			Name:         "store.MarkIndexErrored",
			MetricLabels: []string{"mark_index_errored"},
			Metrics:      metrics,
		}),
		dequeueIndexOperation: observationContext.Operation(observation.Op{
			Name:         "store.DequeueIndex",
			MetricLabels: []string{"dequeue_index"},
			Metrics:      metrics,
		}),
		requeueIndexOperation: observationContext.Operation(observation.Op{
			Name:         "store.RequeueIndex",
			MetricLabels: []string{"requeue_index"},
			Metrics:      metrics,
		}),
		deleteIndexByIdOperation: observationContext.Operation(observation.Op{
			Name:         "store.DeleteIndexByID",
			MetricLabels: []string{"delete_index_by_id"},
			Metrics:      metrics,
		}),
		resetStalledIndexesOperation: observationContext.Operation(observation.Op{
			Name:         "store.ResetStalledIndexes",
			MetricLabels: []string{"reset_stalled_indexes"},
			Metrics:      metrics,
		}),
		repoUsageStatisticsOperation: observationContext.Operation(observation.Op{
			Name:         "store.RepoUsageStatistics",
			MetricLabels: []string{"repo_usage_statistics"},
			Metrics:      metrics,
		}),
		repoNameOperation: observationContext.Operation(observation.Op{
			Name:         "store.RepoName",
			MetricLabels: []string{"repo_name"},
			Metrics:      metrics,
		}),
	}
}

// wrap the given store with the same observed operations as the receiver store.
func (s *ObservedStore) wrap(other Store) Store {
	if other == nil {
		return nil
	}

	return &ObservedStore{
		store:                              other,
		savepointOperation:                 s.savepointOperation,
		rollbackToSavepointOperation:       s.rollbackToSavepointOperation,
		doneOperation:                      s.doneOperation,
		getUploadByIDOperation:             s.getUploadByIDOperation,
		getUploadsOperation:                s.getUploadsOperation,
		queueSizeOperation:                 s.queueSizeOperation,
		insertUploadOperation:              s.insertUploadOperation,
		addUploadPartOperation:             s.addUploadPartOperation,
		markQueuedOperation:                s.markQueuedOperation,
		markCompleteOperation:              s.markCompleteOperation,
		markErroredOperation:               s.markErroredOperation,
		dequeueOperation:                   s.dequeueOperation,
		requeueOperation:                   s.requeueOperation,
		getStatesOperation:                 s.getStatesOperation,
		deleteUploadByIDOperation:          s.deleteUploadByIDOperation,
		resetStalledOperation:              s.resetStalledOperation,
		getDumpIDsOperation:                s.getDumpIDsOperation,
		getDumpByIDOperation:               s.getDumpByIDOperation,
		findClosestDumpsOperation:          s.findClosestDumpsOperation,
		deleteOldestDumpOperation:          s.deleteOldestDumpOperation,
		updateDumpsVisibleFromTipOperation: s.updateDumpsVisibleFromTipOperation,
		deleteOverlappingDumpsOperation:    s.deleteOverlappingDumpsOperation,
		getPackageOperation:                s.getPackageOperation,
		updatePackagesOperation:            s.updatePackagesOperation,
		sameRepoPagerOperation:             s.sameRepoPagerOperation,
		updatePackageReferencesOperation:   s.updatePackageReferencesOperation,
		packageReferencePagerOperation:     s.packageReferencePagerOperation,
		hasCommitOperation:                 s.hasCommitOperation,
		updateCommitsOperation:             s.updateCommitsOperation,
		indexableRepositoriesOperation:     s.indexableRepositoriesOperation,
		updateIndexableRepositoryOperation: s.updateIndexableRepositoryOperation,
		getIndexByIDOperation:              s.getIndexByIDOperation,
		getIndexesOperation:                s.getIndexesOperation,
		indexQueueSizeOperation:            s.indexQueueSizeOperation,
		isQueuedOperation:                  s.isQueuedOperation,
		insertIndexOperation:               s.insertIndexOperation,
		markIndexCompleteOperation:         s.markIndexCompleteOperation,
		markIndexErroredOperation:          s.markIndexErroredOperation,
		dequeueIndexOperation:              s.dequeueIndexOperation,
		requeueIndexOperation:              s.requeueIndexOperation,
		deleteIndexByIdOperation:           s.deleteIndexByIdOperation,
		resetStalledIndexesOperation:       s.resetStalledIndexesOperation,
		repoUsageStatisticsOperation:       s.repoUsageStatisticsOperation,
		repoNameOperation:                  s.repoNameOperation,
	}
}

// Transact calls into the inner store and wraps the resulting value in an ObservedStore.
func (s *ObservedStore) Transact(ctx context.Context) (Store, error) {
	tx, err := s.store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return s.wrap(tx), nil
}

// Savepoint calls into the inner store and registers the observed results.
func (s *ObservedStore) Savepoint(ctx context.Context) (_ string, err error) {
	ctx, endObservation := s.savepointOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.Savepoint(ctx)
}

// RollbackToSavepoint calls into the inner store and registers the observed results.
func (s *ObservedStore) RollbackToSavepoint(ctx context.Context, name string) (err error) {
	ctx, endObservation := s.rollbackToSavepointOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.RollbackToSavepoint(ctx, name)
}

// Done calls into the inner store and registers the observed results.
func (s *ObservedStore) Done(e error) error {
	var observedErr error = nil
	_, endObservation := s.doneOperation.With(context.Background(), &observedErr, observation.Args{})
	defer endObservation(1, observation.Args{})

	err := s.store.Done(e)
	if err != e {
		// Only observe the error if it's a commit/rollback failure
		observedErr = err
	}
	return err
}

// GetUploadByID calls into the inner store and registers the observed results.
func (s *ObservedStore) GetUploadByID(ctx context.Context, id int) (_ Upload, _ bool, err error) {
	ctx, endObservation := s.getUploadByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.GetUploadByID(ctx, id)
}

// GetUploads calls into the inner store and registers the observed results.
func (s *ObservedStore) GetUploads(ctx context.Context, opts GetUploadsOptions) (uploads []Upload, _ int, err error) {
	ctx, endObservation := s.getUploadsOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(uploads)), observation.Args{}) }()
	return s.store.GetUploads(ctx, opts)
}

// QueueSize  calls into the inner store and registers the observed results.
func (s *ObservedStore) QueueSize(ctx context.Context) (_ int, err error) {
	ctx, endObservation := s.queueSizeOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.QueueSize(ctx)
}

// InsertUpload calls into the inner store and registers the observed result.
func (s *ObservedStore) InsertUpload(ctx context.Context, upload Upload) (_ int, err error) {
	ctx, endObservation := s.insertUploadOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.InsertUpload(ctx, upload)
}

// AddUploadPart calls into the inner store and registers the observed result.
func (s *ObservedStore) AddUploadPart(ctx context.Context, uploadID, partIndex int) (err error) {
	ctx, endObservation := s.addUploadPartOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.AddUploadPart(ctx, uploadID, partIndex)
}

// MarkQueued calls into the inner store and registers the observed result.
func (s *ObservedStore) MarkQueued(ctx context.Context, uploadID int) (err error) {
	ctx, endObservation := s.markQueuedOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.MarkQueued(ctx, uploadID)
}

// MarkComplete calls into the inner store and registers the observed results.
func (s *ObservedStore) MarkComplete(ctx context.Context, id int) (err error) {
	ctx, endObservation := s.markCompleteOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.MarkComplete(ctx, id)
}

// MarkErrored calls into the inner store and registers the observed results.
func (s *ObservedStore) MarkErrored(ctx context.Context, id int, failureMessage string) (err error) {
	ctx, endObservation := s.markErroredOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.MarkErrored(ctx, id, failureMessage)
}

// Dequeue calls into the inner store and registers the observed results.
func (s *ObservedStore) Dequeue(ctx context.Context) (_ Upload, _ Store, _ bool, err error) {
	ctx, endObservation := s.dequeueOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	upload, tx, ok, err := s.store.Dequeue(ctx)
	return upload, s.wrap(tx), ok, err
}

// Requeue calls into the inner store and registers the observed results.
func (s *ObservedStore) Requeue(ctx context.Context, id int, after time.Time) (err error) {
	ctx, endObservation := s.requeueOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.Requeue(ctx, id, after)
}

// GetStates calls into the inner store and registers the observed results.
func (s *ObservedStore) GetStates(ctx context.Context, ids []int) (states map[int]string, err error) {
	ctx, endObservation := s.getStatesOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(states)), observation.Args{}) }()
	return s.store.GetStates(ctx, ids)
}

// DeleteUploadByID calls into the inner store and registers the observed results.
func (s *ObservedStore) DeleteUploadByID(ctx context.Context, id int, getTipCommit GetTipCommitFn) (_ bool, err error) {
	ctx, endObservation := s.deleteUploadByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.DeleteUploadByID(ctx, id, getTipCommit)
}

// ResetStalled calls into the inner store and registers the observed results.
func (s *ObservedStore) ResetStalled(ctx context.Context, now time.Time) (resetIDs, erroredIDs []int, err error) {
	ctx, endObservation := s.resetStalledOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(resetIDs)+len(erroredIDs)), observation.Args{}) }()
	return s.store.ResetStalled(ctx, now)
}

// GetDumpIDs calls into the inner store and registers the observed results.
func (s *ObservedStore) GetDumpIDs(ctx context.Context) (ids []int, err error) {
	ctx, endObservation := s.getDumpIDsOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(ids)), observation.Args{}) }()
	return s.store.GetDumpIDs(ctx)
}

// GetDumpByID calls into the inner store and registers the observed results.
func (s *ObservedStore) GetDumpByID(ctx context.Context, id int) (_ Dump, _ bool, err error) {
	ctx, endObservation := s.getDumpByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.GetDumpByID(ctx, id)
}

// FindClosestDumps calls into the inner store and registers the observed results.
func (s *ObservedStore) FindClosestDumps(ctx context.Context, repositoryID int, commit, file, indexer string) (dumps []Dump, err error) {
	ctx, endObservation := s.findClosestDumpsOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(dumps)), observation.Args{}) }()
	return s.store.FindClosestDumps(ctx, repositoryID, commit, file, indexer)
}

// DeleteOldestDump calls into the inner store and registers the observed results.
func (s *ObservedStore) DeleteOldestDump(ctx context.Context) (_ int, _ bool, err error) {
	ctx, endObservation := s.deleteOldestDumpOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.DeleteOldestDump(ctx)
}

// UpdateDumpsVisibleFromTip calls into the inner store and registers the observed results.
func (s *ObservedStore) UpdateDumpsVisibleFromTip(ctx context.Context, repositoryID int, tipCommit string) (err error) {
	ctx, endObservation := s.updateDumpsVisibleFromTipOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.UpdateDumpsVisibleFromTip(ctx, repositoryID, tipCommit)
}

// DeleteOverlappingDumps calls into the inner store and registers the observed results.
func (s *ObservedStore) DeleteOverlappingDumps(ctx context.Context, repositoryID int, commit, root, indexer string) (err error) {
	ctx, endObservation := s.deleteOverlappingDumpsOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.DeleteOverlappingDumps(ctx, repositoryID, commit, root, indexer)
}

// GetPackage calls into the inner store and registers the observed results.
func (s *ObservedStore) GetPackage(ctx context.Context, scheme, name, version string) (_ Dump, _ bool, err error) {
	ctx, endObservation := s.getPackageOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.GetPackage(ctx, scheme, name, version)
}

// UpdatePackages calls into the inner store and registers the observed results.
func (s *ObservedStore) UpdatePackages(ctx context.Context, packages []types.Package) (err error) {
	ctx, endObservation := s.updatePackagesOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.UpdatePackages(ctx, packages)
}

// SameRepoPager calls into the inner store and registers the observed results.
func (s *ObservedStore) SameRepoPager(ctx context.Context, repositoryID int, commit, scheme, name, version string, limit int) (_ int, _ ReferencePager, err error) {
	ctx, endObservation := s.sameRepoPagerOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.SameRepoPager(ctx, repositoryID, commit, scheme, name, version, limit)
}

// UpdatePackageReferences calls into the inner store and registers the observed results.
func (s *ObservedStore) UpdatePackageReferences(ctx context.Context, packageReferences []types.PackageReference) (err error) {
	ctx, endObservation := s.updatePackageReferencesOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.UpdatePackageReferences(ctx, packageReferences)
}

// PackageReferencePager calls into the inner store and registers the observed results.
func (s *ObservedStore) PackageReferencePager(ctx context.Context, scheme, name, version string, repositoryID, limit int) (_ int, _ ReferencePager, err error) {
	ctx, endObservation := s.packageReferencePagerOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.PackageReferencePager(ctx, scheme, name, version, repositoryID, limit)
}

// HasCommit calls into the inner store and registers the observed results.
func (s *ObservedStore) HasCommit(ctx context.Context, repositoryID int, commit string) (_ bool, err error) {
	ctx, endObservation := s.hasCommitOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.HasCommit(ctx, repositoryID, commit)
}

// UpdateCommits calls into the inner store and registers the observed results.
func (s *ObservedStore) UpdateCommits(ctx context.Context, repositoryID int, commits map[string][]string) (err error) {
	ctx, endObservation := s.updateCommitsOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.UpdateCommits(ctx, repositoryID, commits)
}

// IndexableRepositories calls into the inner store and registers the observed results.
func (s *ObservedStore) IndexableRepositories(ctx context.Context, opts IndexableRepositoryQueryOptions) (repos []IndexableRepository, err error) {
	ctx, endObservation := s.indexableRepositoriesOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(repos)), observation.Args{}) }()
	return s.store.IndexableRepositories(ctx, opts)
}

// UpdateIndexableRepository calls into the inner store and registers the observed results.
func (s *ObservedStore) UpdateIndexableRepository(ctx context.Context, indexableRepository UpdateableIndexableRepository) (err error) {
	ctx, endObservation := s.updateIndexableRepositoryOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.UpdateIndexableRepository(ctx, indexableRepository)
}

// GetIndexByID calls into the inner store and registers the observed results.
func (s *ObservedStore) GetIndexByID(ctx context.Context, id int) (_ Index, _ bool, err error) {
	ctx, endObservation := s.getIndexByIDOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.GetIndexByID(ctx, id)
}

// GetIndexes calls into the inner store and registers the observed results.
func (s *ObservedStore) GetIndexes(ctx context.Context, opts GetIndexesOptions) (_ []Index, _ int, err error) {
	ctx, endObservation := s.getIndexesOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.GetIndexes(ctx, opts)
}

// IndexableRepositories calls into the inner store and registers the observed results.
func (s *ObservedStore) IndexQueueSize(ctx context.Context) (_ int, err error) {
	ctx, endObservation := s.indexQueueSizeOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.IndexQueueSize(ctx)
}

// IsQueued calls into the inner store and registers the observed results.
func (s *ObservedStore) IsQueued(ctx context.Context, repositoryID int, commit string) (_ bool, err error) {
	ctx, endObservation := s.isQueuedOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.IsQueued(ctx, repositoryID, commit)
}

// InsertIndex calls into the inner store and registers the observed results.
func (s *ObservedStore) InsertIndex(ctx context.Context, index Index) (_ int, err error) {
	ctx, endObservation := s.insertIndexOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.InsertIndex(ctx, index)
}

// MarkIndexComplete calls into the inner store and registers the observed results.
func (s *ObservedStore) MarkIndexComplete(ctx context.Context, id int) (err error) {
	ctx, endObservation := s.markIndexCompleteOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.MarkIndexComplete(ctx, id)
}

// MarkIndexErrored calls into the inner store and registers the observed results.
func (s *ObservedStore) MarkIndexErrored(ctx context.Context, id int, failureMessage string) (err error) {
	ctx, endObservation := s.markIndexErroredOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.MarkIndexErrored(ctx, id, failureMessage)
}

// DequeueIndex calls into the inner store and registers the observed results.
func (s *ObservedStore) DequeueIndex(ctx context.Context) (_ Index, _ Store, _ bool, err error) {
	ctx, endObservation := s.dequeueIndexOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.DequeueIndex(ctx)
}

// RequeueIndex calls into the inner store and registers the observed results.
func (s *ObservedStore) RequeueIndex(ctx context.Context, id int, after time.Time) (err error) {
	ctx, endObservation := s.requeueIndexOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.RequeueIndex(ctx, id, after)
}

// DeleteIndexByID calls into the inner store and registers the observed results.
func (s *ObservedStore) DeleteIndexByID(ctx context.Context, id int) (_ bool, err error) {
	ctx, endObservation := s.deleteIndexByIdOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.DeleteIndexByID(ctx, id)
}

// ResetStalledIndexes calls into the inner store and registers the observed results.
func (s *ObservedStore) ResetStalledIndexes(ctx context.Context, now time.Time) (resetIDs, erroredIDs []int, err error) {
	ctx, endObservation := s.resetStalledIndexesOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(resetIDs)+len(erroredIDs)), observation.Args{}) }()
	return s.store.ResetStalledIndexes(ctx, now)
}

// RepoUsageStatistics calls into the inner store and registers the observed results.
func (s *ObservedStore) RepoUsageStatistics(ctx context.Context) (stats []RepoUsageStatistics, err error) {
	ctx, endObservation := s.repoUsageStatisticsOperation.With(ctx, &err, observation.Args{})
	defer func() { endObservation(float64(len(stats)), observation.Args{}) }()
	return s.store.RepoUsageStatistics(ctx)
}

// RepoName calls into the inner store and registers the observed results.
func (s *ObservedStore) RepoName(ctx context.Context, repositoryID int) (_ string, err error) {
	ctx, endObservation := s.repoNameOperation.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})
	return s.store.RepoName(ctx, repositoryID)
}
