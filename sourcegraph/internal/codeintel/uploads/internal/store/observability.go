package store

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type operations struct {
	// Not used yet.
	list *observation.Operation

	// Commits
	getStaleSourcedCommits    *observation.Operation
	deleteSourcedCommits      *observation.Operation
	updateSourcedCommits      *observation.Operation
	getCommitsVisibleToUpload *observation.Operation
	getOldestCommitDate       *observation.Operation
	getCommitGraphMetadata    *observation.Operation
	hasCommit                 *observation.Operation

	// Repositories
	getRepositoriesForIndexScan             *observation.Operation
	getRepositoriesMaxStaleAge              *observation.Operation
	getRecentUploadsSummary                 *observation.Operation
	getLastUploadRetentionScanForRepository *observation.Operation
	setRepositoryAsDirty                    *observation.Operation
	setRepositoryAsDirtyWithTx              *observation.Operation
	getDirtyRepositories                    *observation.Operation
	repoName                                *observation.Operation
	setRepositoriesForRetentionScan         *observation.Operation
	hasRepository                           *observation.Operation

	// Uploads
	getUploads                        *observation.Operation
	getUploadByID                     *observation.Operation
	getUploadsByIDs                   *observation.Operation
	getVisibleUploadsMatchingMonikers *observation.Operation
	updateUploadsVisibleToCommits     *observation.Operation
	writeVisibleUploads               *observation.Operation
	persistNearestUploads             *observation.Operation
	persistNearestUploadsLinks        *observation.Operation
	persistUploadsVisibleAtTip        *observation.Operation
	updateUploadRetention             *observation.Operation
	backfillReferenceCountBatch       *observation.Operation
	updateCommittedAt                 *observation.Operation
	sourcedCommitsWithoutCommittedAt  *observation.Operation
	updateUploadsReferenceCounts      *observation.Operation
	deleteUploadsWithoutRepository    *observation.Operation
	deleteUploadsStuckUploading       *observation.Operation
	softDeleteExpiredUploads          *observation.Operation
	hardDeleteUploadsByIDs            *observation.Operation
	deleteUploadByID                  *observation.Operation
	insertUpload                      *observation.Operation
	addUploadPart                     *observation.Operation
	markQueued                        *observation.Operation
	markFailed                        *observation.Operation

	// Dumps
	findClosestDumps                   *observation.Operation
	findClosestDumpsFromGraphFragment  *observation.Operation
	getDumpsWithDefinitionsForMonikers *observation.Operation
	getDumpsByIDs                      *observation.Operation
	deleteOverlappingDumps             *observation.Operation

	// Packages
	updatePackages *observation.Operation

	// References
	updatePackageReferences *observation.Operation
	referencesForUpload     *observation.Operation

	// Audit logs
	deleteOldAuditLogs *observation.Operation

	// Dependencies
	insertDependencySyncingJob *observation.Operation
}

func newOperations(observationContext *observation.Context) *operations {
	metrics := metrics.NewREDMetrics(
		observationContext.Registerer,
		"codeintel_uploads_store",
		metrics.WithLabels("op"),
		metrics.WithCountHelp("Total number of method invocations."),
	)

	op := func(name string) *observation.Operation {
		return observationContext.Operation(observation.Op{
			Name:              fmt.Sprintf("codeintel.uploads.store.%s", name),
			MetricLabelValues: []string{name},
			Metrics:           metrics,
		})
	}

	return &operations{
		// Not used yet.
		list: op("List"),

		// Commits
		getCommitsVisibleToUpload: op("CommitsVisibleToUploads"),
		getOldestCommitDate:       op("GetOldestCommitDate"),
		getStaleSourcedCommits:    op("GetStaleSourcedCommits"),
		getCommitGraphMetadata:    op("GetCommitGraphMetadata"),
		deleteSourcedCommits:      op("DeleteSourcedCommits"),
		updateSourcedCommits:      op("UpdateSourcedCommits"),
		hasCommit:                 op("HasCommit"),

		// Repositories
		getRepositoriesForIndexScan:             op("GetRepositoriesForIndexScan"),
		getRepositoriesMaxStaleAge:              op("GetRepositoriesMaxStaleAge"),
		getRecentUploadsSummary:                 op("GetRecentUploadsSummary"),
		getLastUploadRetentionScanForRepository: op("GetLastUploadRetentionScanForRepository"),
		getDirtyRepositories:                    op("GetDirtyRepositories"),
		setRepositoryAsDirty:                    op("SetRepositoryAsDirty"),
		setRepositoryAsDirtyWithTx:              op("SetRepositoryAsDirtyWithTx"),
		repoName:                                op("RepoName"),
		setRepositoriesForRetentionScan:         op("SetRepositoriesForRetentionScan"),
		hasRepository:                           op("HasRepository"),

		// Uploads
		getUploads:                        op("GetUploads"),
		getUploadByID:                     op("GetUploadByID"),
		getUploadsByIDs:                   op("GetUploadsByIDs"),
		getVisibleUploadsMatchingMonikers: op("GetVisibleUploadsMatchingMonikers"),
		updateUploadsVisibleToCommits:     op("UpdateUploadsVisibleToCommits"),
		updateUploadRetention:             op("UpdateUploadRetention"),
		backfillReferenceCountBatch:       op("BackfillReferenceCountBatch"),
		updateCommittedAt:                 op("UpdateCommittedAt"),
		sourcedCommitsWithoutCommittedAt:  op("SourcedCommitsWithoutCommittedAt"),
		updateUploadsReferenceCounts:      op("UpdateUploadsReferenceCounts"),
		deleteUploadsStuckUploading:       op("DeleteUploadsStuckUploading"),
		deleteUploadsWithoutRepository:    op("DeleteUploadsWithoutRepository"),
		softDeleteExpiredUploads:          op("SoftDeleteExpiredUploads"),
		hardDeleteUploadsByIDs:            op("HardDeleteUploadsByIDs"),
		deleteUploadByID:                  op("DeleteUploadByID"),
		insertUpload:                      op("InsertUpload"),
		addUploadPart:                     op("AddUploadPart"),
		markQueued:                        op("MarkQueued"),
		markFailed:                        op("MarkFailed"),

		writeVisibleUploads:        op("writeVisibleUploads"),
		persistNearestUploads:      op("persistNearestUploads"),
		persistNearestUploadsLinks: op("persistNearestUploadsLinks"),
		persistUploadsVisibleAtTip: op("persistUploadsVisibleAtTip"),

		// Dumps
		findClosestDumps:                   op("FindClosestDumps"),
		findClosestDumpsFromGraphFragment:  op("FindClosestDumpsFromGraphFragment"),
		getDumpsWithDefinitionsForMonikers: op("GetUploadsWithDefinitionsForMonikers"),
		getDumpsByIDs:                      op("GetDumpsByIDs"),
		deleteOverlappingDumps:             op("DeleteOverlappingDumps"),

		// Packages
		updatePackages: op("UpdatePackages"),

		// References
		updatePackageReferences: op("UpdatePackageReferences"),
		referencesForUpload:     op("ReferencesForUpload"),

		// Audit logs
		deleteOldAuditLogs: op("DeleteOldAuditLogs"),

		// Dependencies
		insertDependencySyncingJob: op("InsertDependencySyncingJob"),
	}
}
