package expiration

import (
	"time"

	"github.com/sourcegraph/sourcegraph/internal/goroutine"
)

type UploadService interface {
	NewExpirer(interval time.Duration,
		repositoryProcessDelay time.Duration,
		repositoryBatchSize int,
		uploadProcessDelay time.Duration,
		uploadBatchSize int,
		commitBatchSize int,
		policyBatchSize int,
	) goroutine.BackgroundRoutine
	NewReferenceCountUpdater(interval time.Duration, batchSize int) goroutine.BackgroundRoutine
}
