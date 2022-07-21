package janitor

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const changesetCleanInterval = 24 * time.Hour

// NewChangesetDetachedCleaner creates a new goroutine.PeriodicGoroutine that deletes Changesets that have been
// detached for a period of time.
func NewChangesetDetachedCleaner(ctx context.Context, s *store.Store) goroutine.BackgroundRoutine {
	return goroutine.NewPeriodicGoroutine(
		ctx,
		changesetCleanInterval,
		goroutine.NewHandlerWithErrorMessage("cleaning detached changeset entries", func(ctx context.Context) error {
			// get the configuration value when the handler runs to get the latest value
			retention := conf.Get().BatchChangesChangesetsRetention
			if len(retention) > 0 {
				d, err := time.ParseDuration(retention)
				if err != nil {
					return errors.Wrap(err, "failed to parse config value batchChanges.changesetsRetention as duration")
				}
				return s.CleanDetachedChangesets(ctx, d)
			}
			// nothing to do
			return nil
		}),
	)
}
