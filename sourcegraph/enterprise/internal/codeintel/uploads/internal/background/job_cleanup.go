package background

import (
	"context"
	"sort"
	"time"

	"github.com/derision-test/glock"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/internal/lsifstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/internal/store"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type JanitorConfig struct {
	UploadTimeout                  time.Duration
	AuditLogMaxAge                 time.Duration
	UnreferencedDocumentBatchSize  int
	UnreferencedDocumentMaxAge     time.Duration
	MinimumTimeSinceLastCheck      time.Duration
	CommitResolverBatchSize        int
	CommitResolverMaximumCommitLag time.Duration
}

type janitorJob struct {
	store           store.Store
	lsifStore       lsifstore.LsifStore
	logger          log.Logger
	metrics         *janitorMetrics
	clock           glock.Clock
	gitserverClient GitserverClient
}

func NewJanitor(
	store store.Store,
	lsifStore lsifstore.LsifStore,
	gitserverClient GitserverClient,
	interval time.Duration,
	config JanitorConfig,
	clock glock.Clock,
	logger log.Logger,
	metrics *janitorMetrics,
) goroutine.BackgroundRoutine {
	j := janitorJob{
		store:           store,
		lsifStore:       lsifStore,
		logger:          logger,
		metrics:         metrics,
		clock:           clock,
		gitserverClient: gitserverClient,
	}
	return goroutine.NewPeriodicGoroutine(
		context.Background(),
		"codeintel.upload-janitor", "cleans up various code intel upload and metadata",
		interval,
		goroutine.HandlerFunc(func(ctx context.Context) error {
			return j.handleCleanup(ctx, config)
		}),
	)
}

func (b janitorJob) handleCleanup(ctx context.Context, cfg JanitorConfig) (errs error) {
	// Reconciliation and denormalization
	if err := b.handleDeletedRepository(ctx); err != nil {
		errs = errors.Append(errs, err)
	}
	if err := b.handleUnknownCommit(ctx, cfg); err != nil {
		errs = errors.Append(errs, err)
	}

	// Expiration
	if err := b.handleAbandonedUpload(ctx, cfg); err != nil {
		errs = errors.Append(errs, err)
	}
	if err := b.handleExpiredUploadDeleter(ctx); err != nil {
		errs = errors.Append(errs, err)
	}
	if err := b.handleHardDeleter(ctx); err != nil {
		errs = errors.Append(errs, err)
	}
	if err := b.handleAuditLog(ctx, cfg); err != nil {
		errs = errors.Append(errs, err)
	}

	// SCIP data
	if err := b.handleSCIPDocuments(ctx, cfg); err != nil {
		errs = errors.Append(errs, err)
	}

	return errs
}

func (b janitorJob) handleDeletedRepository(ctx context.Context) (err error) {
	uploadsCounts, err := b.store.DeleteUploadsWithoutRepository(ctx, time.Now())
	if err != nil {
		return errors.Wrap(err, "uploadSvc.DeleteUploadsWithoutRepository")
	}

	for _, counts := range gatherCounts(uploadsCounts) {
		b.logger.Debug(
			"Deleted codeintel records with a deleted repository",
			log.Int("repository_id", counts.repoID),
			log.Int("uploads_count", counts.uploadsCount),
		)

		b.metrics.numUploadRecordsRemoved.Add(float64(counts.uploadsCount))
	}

	return nil
}

type recordCount struct {
	repoID       int
	uploadsCount int
}

func gatherCounts(uploadsCounts map[int]int) []recordCount {
	repoIDsMap := map[int]struct{}{}
	for repoID := range uploadsCounts {
		repoIDsMap[repoID] = struct{}{}
	}

	var repoIDs []int
	for repoID := range repoIDsMap {
		repoIDs = append(repoIDs, repoID)
	}
	sort.Ints(repoIDs)

	recordCounts := make([]recordCount, 0, len(repoIDs))
	for _, repoID := range repoIDs {
		recordCounts = append(recordCounts, recordCount{
			repoID:       repoID,
			uploadsCount: uploadsCounts[repoID],
		})
	}

	return recordCounts
}

func (b janitorJob) handleUnknownCommit(ctx context.Context, cfg JanitorConfig) (err error) {
	staleUploads, err := b.store.GetStaleSourcedCommits(ctx, cfg.MinimumTimeSinceLastCheck, cfg.CommitResolverBatchSize, b.clock.Now())
	if err != nil {
		return errors.Wrap(err, "uploadSvc.StaleSourcedCommits")
	}

	for _, sourcedCommits := range staleUploads {
		if err := b.handleSourcedCommits(ctx, sourcedCommits, cfg); err != nil {
			return err
		}
	}

	return nil
}

func (b janitorJob) handleSourcedCommits(ctx context.Context, sc shared.SourcedCommits, cfg JanitorConfig) error {
	for _, commit := range sc.Commits {
		if err := b.handleCommit(ctx, sc.RepositoryID, commit, cfg); err != nil {
			return err
		}
	}

	return nil
}

func (b janitorJob) handleCommit(ctx context.Context, repositoryID int, commit string, cfg JanitorConfig) error {
	var shouldDelete bool
	_, err := b.gitserverClient.ResolveRevision(ctx, repositoryID, commit)
	if err == nil {
		// If we have no error then the commit is resolvable and we shouldn't touch it.
		shouldDelete = false
	} else if gitdomain.IsRepoNotExist(err) {
		// If we have a repository not found error, then we'll just update the timestamp
		// of the record so we can move on to other data; we deleted records associated
		// with deleted repositories in a separate janitor process.
		shouldDelete = false
	} else if errors.HasType(err, &gitdomain.RevisionNotFoundError{}) {
		// Target condition: repository is resolvable bu the commit is not; was probably
		// force-pushed away and the commit was gc'd after some time or after a re-clone
		// in gitserver.
		shouldDelete = true
	} else {
		// unexpected error
		return errors.Wrap(err, "git.ResolveRevision")
	}

	if shouldDelete {
		_, uploadsDeleted, err := b.store.DeleteSourcedCommits(ctx, repositoryID, commit, cfg.CommitResolverMaximumCommitLag, b.clock.Now())
		if err != nil {
			return errors.Wrap(err, "uploadSvc.DeleteSourcedCommits")
		}
		if uploadsDeleted > 0 {
			b.metrics.numUploadRecordsRemoved.Add(float64(uploadsDeleted))
		}

		return nil
	}

	if _, err := b.store.UpdateSourcedCommits(ctx, repositoryID, commit, b.clock.Now()); err != nil {
		return errors.Wrap(err, "uploadSvc.UpdateSourcedCommits")
	}

	return nil
}

// handleAbandonedUpload removes upload records which have not left the uploading state within the given TTL.
func (b janitorJob) handleAbandonedUpload(ctx context.Context, cfg JanitorConfig) error {
	count, err := b.store.DeleteUploadsStuckUploading(ctx, time.Now().UTC().Add(-cfg.UploadTimeout))
	if err != nil {
		return errors.Wrap(err, "uploadSvc.DeleteUploadsStuckUploading")
	}
	if count > 0 {
		b.logger.Debug("Deleted abandoned upload records", log.Int("count", count))
		b.metrics.numUploadRecordsRemoved.Add(float64(count))
	}

	return nil
}

const (
	expiredUploadsBatchSize    = 1000
	expiredUploadsMaxTraversal = 100
)

func (b janitorJob) handleExpiredUploadDeleter(ctx context.Context) error {
	count, err := b.store.SoftDeleteExpiredUploads(ctx, expiredUploadsBatchSize)
	if err != nil {
		return errors.Wrap(err, "SoftDeleteExpiredUploads")
	}
	if count > 0 {
		b.logger.Info("Deleted expired codeintel uploads", log.Int("count", count))
		b.metrics.numUploadRecordsRemoved.Add(float64(count))
	}

	count, err = b.store.SoftDeleteExpiredUploadsViaTraversal(ctx, expiredUploadsMaxTraversal)
	if err != nil {
		return errors.Wrap(err, "SoftDeleteExpiredUploadsViaTraversal")
	}
	if count > 0 {
		b.logger.Info("Deleted expired codeintel uploads via traversal", log.Int("count", count))
		b.metrics.numUploadRecordsRemoved.Add(float64(count))
	}

	return nil
}

func (b janitorJob) handleHardDeleter(ctx context.Context) error {
	count, err := b.hardDeleteExpiredUploads(ctx)
	if err != nil {
		return errors.Wrap(err, "uploadSvc.HardDeleteExpiredUploads")
	}

	b.metrics.numUploadsPurged.Add(float64(count))
	return nil
}

func (b janitorJob) hardDeleteExpiredUploads(ctx context.Context) (count int, err error) {
	const uploadsBatchSize = 100
	options := shared.GetUploadsOptions{
		State:            "deleted",
		Limit:            uploadsBatchSize,
		AllowExpired:     true,
		AllowDeletedRepo: true,
	}

	for {
		// Always request the first page of deleted uploads. If this is not
		// the first iteration of the loop, then the previous iteration has
		// deleted the records that composed the previous page, and the
		// previous "second" page is now the first page.
		uploads, totalCount, err := b.store.GetUploads(ctx, options)
		if err != nil {
			return 0, errors.Wrap(err, "store.GetUploads")
		}

		ids := uploadIDs(uploads)
		if err := b.lsifStore.DeleteLsifDataByUploadIds(ctx, ids...); err != nil {
			return 0, errors.Wrap(err, "lsifstore.Clear")
		}

		if err := b.store.HardDeleteUploadsByIDs(ctx, ids...); err != nil {
			return 0, errors.Wrap(err, "store.HardDeleteUploadsByIDs")
		}

		count += len(uploads)
		if count >= totalCount {
			break
		}
	}

	return count, nil
}

func (b janitorJob) handleAuditLog(ctx context.Context, cfg JanitorConfig) (err error) {
	count, err := b.store.DeleteOldAuditLogs(ctx, cfg.AuditLogMaxAge, time.Now())
	if err != nil {
		return errors.Wrap(err, "uploadSvc.DeleteOldAuditLogs")
	}

	b.metrics.numAuditLogRecordsExpired.Add(float64(count))
	return nil
}

func (b janitorJob) handleSCIPDocuments(ctx context.Context, cfg JanitorConfig) (err error) {
	count, err := b.lsifStore.DeleteUnreferencedDocuments(ctx, cfg.UnreferencedDocumentBatchSize, cfg.UnreferencedDocumentMaxAge, time.Now())
	if err != nil {
		return errors.Wrap(err, "uploadSvc.DeleteUnreferencedDocuments")
	}

	b.metrics.numSCIPDocumentRecordsRemoved.Add(float64(count))
	return nil
}

func uploadIDs(uploads []types.Upload) []int {
	ids := make([]int, 0, len(uploads))
	for i := range uploads {
		ids = append(ids, uploads[i].ID)
	}
	sort.Ints(ids)

	return ids
}
