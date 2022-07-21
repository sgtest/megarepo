package autoindexing

import (
	"context"
	"os"

	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/api"
	store "github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/autoindex/config"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type IndexEnqueuer = Service

// InferIndexConfiguration looks at the repository contents at the lastest commit on the default branch of the given
// repository and determines an index configuration that is likely to succeed.
func (s *IndexEnqueuer) InferIndexConfiguration(ctx context.Context, repositoryID int, commit string, bypassLimit bool) (_ *config.IndexConfiguration, hints []config.IndexJobHint, err error) {
	ctx, trace, endObservation := s.operations.inferIndexConfiguration.With(ctx, &err, observation.Args{
		LogFields: []otlog.Field{
			otlog.Int("repositoryID", repositoryID),
		},
	})
	defer endObservation(1, observation.Args{})

	if commit == "" {
		var ok bool
		commit, ok, err = s.gitserverClient.Head(ctx, repositoryID)
		if err != nil || !ok {
			return nil, nil, errors.Wrapf(err, "gitserver.Head: error resolving HEAD for %d", repositoryID)
		}
	} else {
		exists, err := s.gitserverClient.CommitExists(ctx, repositoryID, commit)
		if err != nil {
			return nil, nil, errors.Wrapf(err, "gitserver.CommitExists: error checking %s for %d", commit, repositoryID)
		}

		if !exists {
			return nil, nil, errors.Newf("revision %s not found for %d", commit, repositoryID)
		}
	}
	trace.Log(otlog.String("commit", commit))

	indexJobs, err := s.inferIndexJobsFromRepositoryStructure(ctx, repositoryID, commit, bypassLimit)
	if err != nil {
		return nil, nil, err
	}

	indexJobHints, err := s.inferIndexJobHintsFromRepositoryStructure(ctx, repositoryID, commit)
	if err != nil {
		return nil, nil, err
	}

	if len(indexJobs) == 0 {
		return nil, indexJobHints, nil
	}

	return &config.IndexConfiguration{
		IndexJobs: indexJobs,
	}, indexJobHints, nil
}

// QueueIndexes enqueues a set of index jobs for the following repository and commit. If a non-empty
// configuration is given, it will be used to determine the set of jobs to enqueue. Otherwise, it will
// the configuration will be determined based on the regular index scheduling rules: first read any
// in-repo configuration (e.g., sourcegraph.yaml), then look for any existing in-database configuration,
// finally falling back to the automatically inferred connfiguration based on the repo contents at the
// target commit.
//
// If the force flag is false, then the presence of an upload or index record for this given repository and commit
// will cause this method to no-op. Note that this is NOT a guarantee that there will never be any duplicate records
// when the flag is false.
func (s *IndexEnqueuer) QueueIndexes(ctx context.Context, repositoryID int, rev, configuration string, force, bypassLimit bool) (_ []store.Index, err error) {
	ctx, trace, endObservation := s.operations.queueIndex.With(ctx, &err, observation.Args{
		LogFields: []otlog.Field{
			otlog.Int("repositoryID", repositoryID),
		},
	})
	defer endObservation(1, observation.Args{})

	commitID, err := s.gitserverClient.ResolveRevision(ctx, repositoryID, rev)
	if err != nil {
		return nil, errors.Wrap(err, "gitserver.ResolveRevision")
	}
	commit := string(commitID)
	trace.Log(otlog.String("commit", commit))

	return s.queueIndexForRepositoryAndCommit(ctx, repositoryID, commit, configuration, force, bypassLimit, nil) // trace)
}

// QueueIndexesForPackage enqueues index jobs for a dependency of a recently-processed precise code
// intelligence index.
func (s *IndexEnqueuer) QueueIndexesForPackage(ctx context.Context, pkg precise.Package) (err error) {
	ctx, trace, endObservation := s.operations.queueIndexForPackage.With(ctx, &err, observation.Args{
		LogFields: []otlog.Field{
			otlog.String("scheme", pkg.Scheme),
			otlog.String("name", pkg.Name),
			otlog.String("version", pkg.Version),
		},
	})
	defer endObservation(1, observation.Args{})

	repoName, revision, ok := InferRepositoryAndRevision(pkg)
	if !ok {
		return nil
	}
	trace.Log(otlog.String("repoName", string(repoName)))
	trace.Log(otlog.String("revision", revision))

	// if err := s.repoUpdaterLimiter.Wait(ctx); err != nil {
	// 	return err
	// }

	resp, err := s.repoUpdater.EnqueueRepoUpdate(ctx, repoName)
	if err != nil {
		if errcode.IsNotFound(err) {
			return nil
		}

		return errors.Wrap(err, "repoUpdater.EnqueueRepoUpdate")
	}

	commit, err := s.gitserverClient.ResolveRevision(ctx, int(resp.ID), revision)
	if err != nil {
		if errcode.IsNotFound(err) {
			return nil
		}

		return errors.Wrap(err, "gitserverClient.ResolveRevision")
	}

	_, err = s.queueIndexForRepositoryAndCommit(ctx, int(resp.ID), string(commit), "", false, false, nil) // trace)
	return err
}

// queueIndexForRepositoryAndCommit determines a set of index jobs to enqueue for the given repository and commit.
//
// If the force flag is false, then the presence of an upload or index record for this given repository and commit
// will cause this method to no-op. Note that this is NOT a guarantee that there will never be any duplicate records
// when the flag is false.
func (s *IndexEnqueuer) queueIndexForRepositoryAndCommit(ctx context.Context, repositoryID int, commit, configuration string, force, bypassLimit bool, trace observation.TraceLogger) ([]store.Index, error) {
	if !force {
		isQueued, err := s.dbStore.IsQueued(ctx, repositoryID, commit)
		if err != nil {
			return nil, errors.Wrap(err, "dbstore.IsQueued")
		}
		if isQueued {
			return nil, nil
		}
	}

	indexes, err := s.getIndexRecords(ctx, repositoryID, commit, configuration, bypassLimit)
	if err != nil {
		return nil, err
	}
	if len(indexes) == 0 {
		return nil, nil
	}
	// trace.Log(log.Int("numIndexes", len(indexes)))

	return s.dbStore.InsertIndexes(ctx, indexes)
}

var overrideScript = os.Getenv("SRC_CODEINTEL_INFERENCE_OVERRIDE_SCRIPT")

// inferIndexJobsFromRepositoryStructure collects the result of  InferIndexJobs over all registered recognizers.
func (s *IndexEnqueuer) inferIndexJobsFromRepositoryStructure(ctx context.Context, repositoryID int, commit string, bypassLimit bool) ([]config.IndexJob, error) {
	// if err := s.gitserverLimiter.Wait(ctx); err != nil {
	// 	return nil, err
	// }

	repoName, err := s.dbStore.RepoName(ctx, repositoryID)
	if err != nil {
		return nil, err
	}

	indexes, err := s.inferenceService.InferIndexJobs(ctx, api.RepoName(repoName), commit, overrideScript)
	if err != nil {
		return nil, err
	}

	if !bypassLimit && len(indexes) > maximumIndexJobsPerInferredConfiguration {
		s.logger.Info("Too many inferred roots. Scheduling no index jobs for repository.", log.Int("repository_id", repositoryID))
		return nil, nil
	}

	return indexes, nil
}

// inferIndexJobsFromRepositoryStructure collects the result of  InferIndexJobHints over all registered recognizers.
func (s *IndexEnqueuer) inferIndexJobHintsFromRepositoryStructure(ctx context.Context, repositoryID int, commit string) ([]config.IndexJobHint, error) {
	// if err := s.gitserverLimiter.Wait(ctx); err != nil {
	// 	return nil, err
	// }

	repoName, err := s.dbStore.RepoName(ctx, repositoryID)
	if err != nil {
		return nil, err
	}

	indexes, err := s.inferenceService.InferIndexJobHints(ctx, api.RepoName(repoName), commit, overrideScript)
	if err != nil {
		return nil, err
	}

	return indexes, nil
}
