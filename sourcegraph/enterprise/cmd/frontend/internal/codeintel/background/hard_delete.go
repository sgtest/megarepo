package background

import (
	"context"
	"sort"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	lsifstore "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/lsifstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
)

type HardDeleter struct {
	store     store.Store
	lsifStore lsifstore.Store
	metrics   Metrics
}

var _ goroutine.Handler = &HardDeleter{}

// NewHardDeleter returns a background routine that periodically hard-deletes all
// soft-deleted upload records. Each upload record marked as soft-deleted in the
// database will have its associated data in the code intel deleted, and the upload
// record hard-deleted.
//
// This cleanup routine subsumes an old routine that would remove any records which
// did not have an associated upload record. Doing a soft-delete and a transactional
// cleanup routine instead ensures we delete unreachable data as soon as it's no longer
// referenceable.
func NewHardDeleter(store store.Store, lsifStore lsifstore.Store, interval time.Duration, metrics Metrics) goroutine.BackgroundRoutine {
	return goroutine.NewPeriodicGoroutine(context.Background(), interval, &HardDeleter{
		store:     store,
		lsifStore: lsifStore,
		metrics:   metrics,
	})
}

const uploadsBatchSize = 100

func (d *HardDeleter) Handle(ctx context.Context) error {
	options := store.GetUploadsOptions{
		State: "deleted",
		Limit: uploadsBatchSize,
	}

	for {
		// Always request the first page of deleted uploads. If this is not
		// the first iteration of the loop, then the previous iteration has
		// deleted the records that composed the previous page, and the
		// previous "second" page is now the first page.
		uploads, totalCount, err := d.store.GetUploads(ctx, options)
		if err != nil {
			return errors.Wrap(err, "GetUploads")
		}

		if err := d.deleteBatch(ctx, uploadIDs(uploads)); err != nil {
			return err
		}

		count := len(uploads)
		log15.Debug("Deleted data associated with uploads", "upload_count", count)
		d.metrics.UploadDataRemoved.Add(float64(count))

		if count >= totalCount {
			break
		}
	}

	return nil
}

func (d *HardDeleter) HandleError(err error) {
	d.metrics.Errors.Inc()
	log15.Error("Failed to hard delete upload records", "error", err)
}

func (d *HardDeleter) deleteBatch(ctx context.Context, ids []int) (err error) {
	tx, err := d.store.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	if err := d.lsifStore.Clear(ctx, ids...); err != nil {
		return errors.Wrap(err, "Clear")
	}

	if err := tx.HardDeleteUploadByID(ctx, ids...); err != nil {
		return errors.Wrap(err, "HardDeleteUploadByID")
	}

	return nil
}

func uploadIDs(uploads []store.Upload) []int {
	ids := make([]int, 0, len(uploads))
	for i := range uploads {
		ids = append(ids, uploads[i].ID)
	}
	sort.Ints(ids)

	return ids
}
