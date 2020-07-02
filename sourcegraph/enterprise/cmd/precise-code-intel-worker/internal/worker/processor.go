package worker

import (
	"context"
	"io/ioutil"
	"os"
	"path/filepath"
	"time"

	"github.com/hashicorp/go-multierror"
	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/metrics"
	bundles "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/client"
	sqlitewriter "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/persistence/sqlite"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
)

// CloneInProgressDelay is the delay between processing attempts when a repo is currently being cloned.
const CloneInProgressDelay = time.Minute

// Processor converts raw uploads into dumps.
type Processor interface {
	Process(ctx context.Context, tx store.Store, upload store.Upload) (bool, error)
}

type processor struct {
	bundleManagerClient bundles.BundleManagerClient
	gitserverClient     gitserver.Client
	metrics             metrics.WorkerMetrics
}

// process converts a raw upload into a dump within the given transaction context. Returns true if the
// upload record was requeued and false otherwise.
func (p *processor) Process(ctx context.Context, store store.Store, upload store.Upload) (_ bool, err error) {
	// Ensure that the repo and revision are resolvable. If the repo does not exist, or if the repo has finished
	// cloning and the revision does not exist, then the upload will fail to process. If the repo is currently
	// cloning, then we'll requeue the upload to be tried again later. This will not increase the reset count
	// of the record (so this doesn't count against the upload as a legitimate attempt).
	if cloneInProgress, err := p.isRepoCurrentlyCloning(ctx, upload.RepositoryID, upload.Commit); err != nil {
		return false, err
	} else if cloneInProgress {
		if err := store.Requeue(ctx, upload.ID, time.Now().UTC().Add(CloneInProgressDelay)); err != nil {
			return false, errors.Wrap(err, "store.Requeue")
		}

		return true, nil
	}

	// Create scratch directory that we can clean on completion/failure
	tempDir, err := ioutil.TempDir("", "")
	if err != nil {
		return false, err
	}
	defer func() {
		if cleanupErr := os.RemoveAll(tempDir); cleanupErr != nil {
			log15.Warn("Failed to remove temporary directory", "path", tempDir, "err", cleanupErr)
		}
	}()

	// Pull raw uploaded data from bundle manager
	r, err := p.bundleManagerClient.GetUpload(ctx, upload.ID)
	if err != nil {
		return false, errors.Wrap(err, "bundleManager.GetUpload")
	}
	defer func() {
		if err != nil {
			// Remove upload file on error instead of waiting for it to expire
			if deleteErr := p.bundleManagerClient.DeleteUpload(ctx, upload.ID); deleteErr != nil {
				log15.Warn("Failed to delete upload file", "err", err)
			}
		}
	}()

	getChildren := func(ctx context.Context, dirnames []string) (map[string][]string, error) {
		directoryChildren, err := p.gitserverClient.DirectoryChildren(ctx, store, upload.RepositoryID, upload.Commit, dirnames)
		if err != nil {
			return nil, errors.Wrap(err, "gitserverClient.DirectoryChildren")
		}
		return directoryChildren, nil
	}

	groupedBundleData, err := correlation.Correlate(ctx, r, upload.ID, upload.Root, getChildren, p.metrics)
	if err != nil {
		return false, errors.Wrap(err, "correlation.Correlate")
	}

	if err := p.write(ctx, tempDir, groupedBundleData); err != nil {
		return false, err
	}

	// At this point we haven't touched the database. We're going to start a nested transaction
	// with Postgres savepoints. In the event that something after this point fails, we want to
	// update the upload record with an error message but do not want to alter any other data in
	// the database. Rolling back to this savepoint will allow us to discard any other changes
	// but still commit the transaction as a whole.
	savepointID, err := store.Savepoint(ctx)
	if err != nil {
		return false, errors.Wrap(err, "store.Savepoint")
	}
	defer func() {
		if err != nil {
			if rollbackErr := store.RollbackToSavepoint(ctx, savepointID); rollbackErr != nil {
				err = multierror.Append(err, rollbackErr)
			}
		}
	}()

	if err := p.updateXrepoData(ctx, store, upload, groupedBundleData.Packages, groupedBundleData.PackageReferences); err != nil {
		return false, err
	}

	// Send converted database file to bundle manager
	if err := p.sendDB(ctx, upload.ID, tempDir); err != nil {
		return false, err
	}

	return false, nil
}

// isRepoCurrentlyCloning determines if the target repository is currently being cloned.
// This function returns an error if the repo or commit cannot be resolved.
func (p *processor) isRepoCurrentlyCloning(ctx context.Context, repoID int, commit string) (_ bool, err error) {
	ctx, endOperation := p.metrics.RepoStateOperation.With(ctx, &err, observation.Args{})
	defer endOperation(1, observation.Args{})

	repo, err := backend.Repos.Get(ctx, api.RepoID(repoID))
	if err != nil {
		return false, errors.Wrap(err, "Repos.Get")
	}

	if _, err := backend.Repos.ResolveRev(ctx, repo, commit); err != nil {
		if vcs.IsCloneInProgress(err) {
			return true, nil
		}

		return false, errors.Wrap(err, "Repos.ResolveRev")
	}

	return false, nil
}

// write commits the correlated data to disk.
func (p *processor) write(ctx context.Context, dirname string, groupedBundleData *correlation.GroupedBundleData) (err error) {
	ctx, endOperation := p.metrics.WriteOperation.With(ctx, &err, observation.Args{})
	defer endOperation(1, observation.Args{})

	writer, err := sqlitewriter.NewWriter(ctx, filepath.Join(dirname, "sqlite.db"))
	if err != nil {
		return err
	}
	defer func() {
		err = writer.Close(err)
	}()

	if err := writer.WriteMeta(ctx, groupedBundleData.Meta); err != nil {
		return errors.Wrap(err, "writer.WriteMeta")
	}
	if err := writer.WriteDocuments(ctx, groupedBundleData.Documents); err != nil {
		return errors.Wrap(err, "writer.WriteDocuments")
	}
	if err := writer.WriteResultChunks(ctx, groupedBundleData.ResultChunks); err != nil {
		return errors.Wrap(err, "writer.WriteResultChunks")
	}
	if err := writer.WriteDefinitions(ctx, groupedBundleData.Definitions); err != nil {
		return errors.Wrap(err, "writer.WriteDefinitions")
	}
	if err := writer.WriteReferences(ctx, groupedBundleData.References); err != nil {
		return errors.Wrap(err, "writer.WriteReferences")
	}

	return err
}

func (p *processor) updateXrepoData(ctx context.Context, store store.Store, upload store.Upload, packages []types.Package, packageReferences []types.PackageReference) (err error) {
	ctx, endOperation := p.metrics.UpdateXrepoDatabaseOperation.With(ctx, &err, observation.Args{})
	defer endOperation(1, observation.Args{})

	// Update package and package reference data to support cross-repo queries.
	if err := store.UpdatePackages(ctx, packages); err != nil {
		return errors.Wrap(err, "store.UpdatePackages")
	}
	if err := store.UpdatePackageReferences(ctx, packageReferences); err != nil {
		return errors.Wrap(err, "store.UpdatePackageReferences")
	}

	// Before we mark the upload as complete, we need to delete any existing completed uploads
	// that have the same repository_id, commit, root, and indexer values. Otherwise the transaction
	// will fail as these values form a unique constraint.
	if err := store.DeleteOverlappingDumps(ctx, upload.RepositoryID, upload.Commit, upload.Root, upload.Indexer); err != nil {
		return errors.Wrap(err, "store.DeleteOverlappingDumps")
	}

	// Almost-success: we need to mark this upload as complete at this point as the next step changes
	// the visibility of the dumps for this repository. This requires that the new dump be available in
	// the lsif_dumps view, which requires a change of state. In the event of a future failure we can
	// still roll back to the save point and mark the upload as errored.
	if err := store.MarkComplete(ctx, upload.ID); err != nil {
		return errors.Wrap(err, "store.MarkComplete")
	}

	// Discover commits around the current tip commit and the commit of this upload. Upsert these
	// commits into the lsif_commits table, then update the visibility of all dumps for this repository.
	tipCommit, err := p.gitserverClient.Head(ctx, store, upload.RepositoryID)
	if err != nil {
		return errors.Wrap(err, "gitserver.Head")
	}
	newCommits, err := p.gitserverClient.CommitsNear(ctx, store, upload.RepositoryID, tipCommit)
	if err != nil {
		return errors.Wrap(err, "gitserver.CommitsNear")
	}

	if tipCommit != upload.Commit {
		// If the tip is ahead of this commit, we also want to discover all of the commits between this
		// commit and the tip so that we can accurately determine what is visible from the tip. If we
		// do not do this before the updateDumpsVisibleFromTip call below, no dumps will be reachable
		// from the tip and all dumps will be invisible.
		additionalCommits, err := p.gitserverClient.CommitsNear(ctx, store, upload.RepositoryID, upload.Commit)
		if err != nil {
			return errors.Wrap(err, "gitserver.CommitsNear")
		}

		for k, vs := range additionalCommits {
			newCommits[k] = append(newCommits[k], vs...)
		}
	}

	if err := store.UpdateCommits(ctx, upload.RepositoryID, newCommits); err != nil {
		return errors.Wrap(err, "store.UpdateCommits")
	}

	if err := store.UpdateDumpsVisibleFromTip(ctx, upload.RepositoryID, tipCommit); err != nil {
		return errors.Wrap(err, "store.UpdateDumpsVisibleFromTip")
	}

	return nil
}

func (p *processor) sendDB(ctx context.Context, uploadID int, tempDir string) (err error) {
	ctx, endOperation := p.metrics.SendDBOperation.With(ctx, &err, observation.Args{})
	defer endOperation(1, observation.Args{})

	if err := p.bundleManagerClient.SendDB(ctx, uploadID, tempDir); err != nil {
		return errors.Wrap(err, "bundleManager.SendDB")
	}

	return nil
}
