package janitor

import (
	"context"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-bundle-manager/internal/paths"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
)

// removeProcessedUploadsWithoutBundleFile removes all processed upload records
// that do not have a corresponding bundle file on disk.
func (j *Janitor) removeProcessedUploadsWithoutBundleFile() error {
	ctx := context.Background()

	getTipCommit := func(repositoryID int) (string, error) {
		tipCommit, err := gitserver.Head(ctx, j.db, repositoryID)
		if err != nil && !isRepoNotExist(err) {
			return "", errors.Wrap(err, "gitserver.Head")
		}
		return tipCommit, nil
	}

	// TODO(efritz) - request in batches
	ids, err := j.db.GetDumpIDs(ctx)
	if err != nil {
		return errors.Wrap(err, "db.GetDumpIDs")
	}

	for _, id := range ids {
		exists, err := paths.PathExists(paths.DBDir(j.bundleDir, int64(id)))
		if err != nil {
			return errors.Wrap(err, "paths.PathExists")
		}
		if exists {
			continue
		}

		deleted, err := j.db.DeleteUploadByID(ctx, id, getTipCommit)
		if err != nil {
			return errors.Wrap(err, "db.DeleteUploadByID")
		}

		if deleted {
			log15.Debug("Removed upload record with no bundle file", "id", id)
			j.metrics.UploadRecordsRemoved.Inc()
		}
	}

	return nil
}

func isRepoNotExist(err error) bool {
	for err != nil {
		if vcs.IsRepoNotExist(err) {
			return true
		}

		err = errors.Unwrap(err)
	}

	return false
}
