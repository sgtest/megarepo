package cleanup

import (
	"time"

	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
)

type UploadServiceBackgroundJobs interface {
	NewJanitor(
		interval time.Duration,
		uploadTimeout time.Duration,
		auditLogMaxAge time.Duration,
		minimumTimeSinceLastCheck time.Duration,
		commitResolverBatchSize int,
		commitResolverMaximumCommitLag time.Duration,
	) goroutine.BackgroundRoutine

	NewReconciler(
		interval time.Duration,
		batchSize int,
	) goroutine.BackgroundRoutine

	NewUploadResetter(interval time.Duration) *dbworker.Resetter
}
