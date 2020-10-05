package scheduler

import (
	"context"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/index"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
)

type Scheduler struct {
	store                       store.Store
	gitserverClient             gitserver.Client
	batchSize                   int
	minimumTimeSinceLastEnqueue time.Duration
	minimumSearchCount          int
	minimumSearchRatio          float64
	minimumPreciseCount         int
	metrics                     SchedulerMetrics
}

var _ goroutine.Handler = &Scheduler{}

func NewScheduler(
	store store.Store,
	gitserverClient gitserver.Client,
	interval time.Duration,
	batchSize int,
	minimumTimeSinceLastEnqueue time.Duration,
	minimumSearchCount int,
	minimumSearchRatio float64,
	minimumPreciseCount int,
	metrics SchedulerMetrics,
) goroutine.BackgroundRoutine {
	return goroutine.NewPeriodicGoroutine(context.Background(), interval, &Scheduler{
		store:                       store,
		gitserverClient:             gitserverClient,
		batchSize:                   batchSize,
		minimumTimeSinceLastEnqueue: minimumTimeSinceLastEnqueue,
		minimumSearchCount:          minimumSearchCount,
		minimumSearchRatio:          minimumSearchRatio,
		minimumPreciseCount:         minimumPreciseCount,
		metrics:                     metrics,
	})
}

func (s *Scheduler) Handle(ctx context.Context) error {
	configuredRepositoryIDs, err := s.store.GetRepositoriesWithIndexConfiguration(ctx)
	if err != nil {
		return errors.Wrap(err, "store.GetRepositoriesWithIndexConfiguration")
	}

	indexableRepositories, err := s.store.IndexableRepositories(ctx, store.IndexableRepositoryQueryOptions{
		Limit:                       s.batchSize,
		MinimumTimeSinceLastEnqueue: s.minimumTimeSinceLastEnqueue,
		MinimumSearchCount:          s.minimumSearchCount,
		MinimumPreciseCount:         s.minimumPreciseCount,
		MinimumSearchRatio:          s.minimumSearchRatio,
	})
	if err != nil {
		return errors.Wrap(err, "store.IndexableRepositories")
	}

	var indexableRepositoryIDs []int
	for _, indexableRepository := range indexableRepositories {
		indexableRepositoryIDs = append(indexableRepositoryIDs, indexableRepository.RepositoryID)
	}

	for _, repositoryID := range deduplicateRepositoryIDs(configuredRepositoryIDs, indexableRepositoryIDs) {
		if err := s.queueIndex(ctx, repositoryID); err != nil {
			if isRepoNotExist(err) {
				continue
			}

			return err
		}
	}

	return nil
}

func (s *Scheduler) HandleError(err error) {
	s.metrics.Errors.Inc()
	log15.Error("Failed to update indexable repositories", "err", err)
}

func (s *Scheduler) queueIndex(ctx context.Context, repositoryID int) (err error) {
	commit, err := s.gitserverClient.Head(ctx, s.store, repositoryID)
	if err != nil {
		return errors.Wrap(err, "gitserver.Head")
	}

	isQueued, err := s.store.IsQueued(ctx, repositoryID, commit)
	if err != nil {
		return errors.Wrap(err, "store.IsQueued")
	}
	if isQueued {
		return nil
	}

	indexes, err := s.inferIndexes(ctx, repositoryID, commit)
	if err != nil {
		return err
	}
	if len(indexes) == 0 {
		return nil
	}

	tx, err := s.store.Transact(ctx)
	if err != nil {
		return errors.Wrap(err, "store.Transact")
	}
	defer func() {
		err = tx.Done(err)
	}()

	for _, index := range indexes {
		id, err := tx.InsertIndex(ctx, index)
		if err != nil {
			return errors.Wrap(err, "store.QueueIndex")
		}

		log15.Info(
			"Enqueued index",
			"id", id,
			"repository_id", repositoryID,
			"commit", commit,
		)
	}

	now := time.Now().UTC()
	update := store.UpdateableIndexableRepository{
		RepositoryID:        repositoryID,
		LastIndexEnqueuedAt: &now,
	}

	// TODO(efritz) - this may create records once a repository has an explicit
	// index configuration. This shouldn't affect any indexing behavior at all.
	if err := tx.UpdateIndexableRepository(ctx, update, now); err != nil {
		return errors.Wrap(err, "store.UpdateIndexableRepository")
	}

	return nil
}

func (s *Scheduler) inferIndexes(ctx context.Context, repositoryID int, commit string) ([]store.Index, error) {
	indexConfigurationRecord, ok, err := s.store.GetIndexConfigurationByRepositoryID(ctx, repositoryID)
	if err != nil {
		return nil, errors.Wrap(err, "store.GetIndexConfigurationByRepositoryID")
	}
	if ok {
		indexConfiguration, err := index.UnmarshalJSON(indexConfigurationRecord.Data)
		if err != nil {
			log15.Warn("Failed to unmarshal index configuration", "repository_id", repositoryID)
			return nil, nil
		}

		var indexes []store.Index
		for _, indexJob := range indexConfiguration.IndexJobs {
			var dockerSteps []store.DockerStep
			for _, dockerStep := range indexConfiguration.SharedSteps {
				dockerSteps = append(dockerSteps, store.DockerStep{
					Root:     dockerStep.Root,
					Image:    dockerStep.Image,
					Commands: dockerStep.Commands,
				})
			}
			for _, dockerStep := range indexJob.Steps {
				dockerSteps = append(dockerSteps, store.DockerStep{
					Root:     dockerStep.Root,
					Image:    dockerStep.Image,
					Commands: dockerStep.Commands,
				})
			}

			indexes = append(indexes, store.Index{
				Commit:       commit,
				RepositoryID: repositoryID,
				State:        "queued",
				DockerSteps:  dockerSteps,
				Root:         indexJob.Root,
				Indexer:      indexJob.Indexer,
				IndexerArgs:  indexJob.IndexerArgs,
				Outfile:      indexJob.Outfile,
			})
		}

		return indexes, nil
	}

	index := store.Index{
		Commit:       commit,
		RepositoryID: repositoryID,
		State:        "queued",
		DockerSteps:  []store.DockerStep{},
		Root:         "",
		Indexer:      "sourcegraph/lsif-go:latest",
		IndexerArgs:  []string{"lsif-go", "--no-animation"},
		Outfile:      "",
	}

	return []store.Index{index}, nil
}

func deduplicateRepositoryIDs(ids ...[]int) (repositoryIDs []int) {
	repositoryIDMap := map[int]struct{}{}
	for _, s := range ids {
		for _, v := range s {
			repositoryIDMap[v] = struct{}{}
		}
	}

	for repositoryID := range repositoryIDMap {
		repositoryIDs = append(repositoryIDs, repositoryID)
	}

	return repositoryIDs
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
