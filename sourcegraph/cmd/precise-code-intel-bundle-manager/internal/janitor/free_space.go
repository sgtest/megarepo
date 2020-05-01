package janitor

import (
	"context"
	"os"
	"syscall"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-bundle-manager/internal/paths"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/lsifserver/client"
)

type PruneFn func(ctx context.Context) (int64, bool, error)

func defaultPruneFn(ctx context.Context) (int64, bool, error) {
	id, prunable, err := client.DefaultClient.Prune(ctx)
	if err != nil {
		return 0, false, errors.Wrap(err, "lsifserver.Prune")
	}

	return id, prunable, nil
}

// freeSpace determines the space available on the device containing the bundle directory,
// then calls cleanOldDumps to free enough space to get back below the disk usage threshold.
func (j *Janitor) freeSpace(pruneFn PruneFn) error {
	var fs syscall.Statfs_t
	if err := syscall.Statfs(j.bundleDir, &fs); err != nil {
		return err
	}

	diskSizeBytes := fs.Blocks * uint64(fs.Bsize)
	freeBytes := fs.Bavail * uint64(fs.Bsize)
	desiredFreeBytes := uint64(float64(diskSizeBytes) * float64(j.desiredPercentFree) / 100.0)

	if freeBytes < desiredFreeBytes {
		return j.cleanOldDumps(pruneFn, uint64(desiredFreeBytes-freeBytes))
	}

	return nil
}

// cleanOldDumps removes dumps from the database (via precise-code-intel-api-server)
// and the filesystem until at least bytesToFree, or there are no more prunable dumps.
func (j *Janitor) cleanOldDumps(pruneFn func(ctx context.Context) (int64, bool, error), bytesToFree uint64) error {
	for bytesToFree > 0 {
		bytesRemoved, pruned, err := j.cleanOldDump(pruneFn)
		if err != nil {
			return err
		}
		if !pruned {
			break
		}

		if bytesRemoved >= bytesToFree {
			break
		}

		bytesToFree -= bytesRemoved
	}

	return nil
}

// cleanOldDump calls the precise-code-intel-api-server for the identifier of
// the oldest dump to remove then deletes the associated file. This method
// returns the size of the deleted file on success, and returns a false-valued
// flag if there are no prunable dumps.
func (j *Janitor) cleanOldDump(pruneFn func(ctx context.Context) (int64, bool, error)) (uint64, bool, error) {
	id, prunable, err := pruneFn(context.Background())
	if err != nil || !prunable {
		return 0, false, err
	}

	filename := paths.DBFilename(j.bundleDir, id)

	fileInfo, err := os.Stat(filename)
	if err != nil {
		return 0, false, err
	}

	if err := os.Remove(filename); err != nil {
		return 0, false, err
	}

	return uint64(fileInfo.Size()), true, nil
}
