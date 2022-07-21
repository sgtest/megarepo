package scheduler

import (
	"context"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// For mocking in tests
var autoIndexingEnabled = conf.CodeIntelAutoIndexingEnabled

func (s *scheduler) handle(ctx context.Context) (err error) {
	if !autoIndexingEnabled() {
		return nil
	}

	var repositoryMatchLimit *int
	if val := conf.CodeIntelAutoIndexingPolicyRepositoryMatchLimit(); val != -1 {
		repositoryMatchLimit = &val
	}

	// Get the batch of repositories that we'll handle in this invocation of the periodic goroutine. This
	// set should contain repositories that have yet to be updated, or that have been updated least recently.
	// This allows us to update every repository reliably, even if it takes a long time to process through
	// the backlog.
	repositories, err := s.dbStore.SelectRepositoriesForIndexScan(
		ctx,
		"lsif_last_index_scan",
		"last_index_scan_at",
		ConfigInst.RepositoryProcessDelay,
		conf.CodeIntelAutoIndexingAllowGlobalPolicies(),
		repositoryMatchLimit,
		ConfigInst.RepositoryBatchSize,
	)
	if err != nil {
		return errors.Wrap(err, "dbstore.SelectRepositoriesForIndexScan")
	}
	if len(repositories) == 0 {
		// All repositories updated recently enough
		return nil
	}

	now := timeutil.Now()

	for _, repositoryID := range repositories {
		if repositoryErr := s.handleRepository(ctx, repositoryID, now); repositoryErr != nil {
			if err == nil {
				err = repositoryErr
			} else {
				err = errors.Append(err, repositoryErr)
			}
		}
	}

	return err
}

func (s *scheduler) handleError(err error) {
	s.logger.Error("Failed to schedule index jobs", log.Error(err))
}

func (s *scheduler) handleRepository(
	ctx context.Context,
	repositoryID int,
	now time.Time,
) error {
	offset := 0

	for {
		// Retrieve the set of configuration policies that affect indexing for this repository.
		policies, totalCount, err := s.dbStore.GetConfigurationPolicies(ctx, dbstore.GetConfigurationPoliciesOptions{
			RepositoryID: repositoryID,
			ForIndexing:  true,
			Limit:        ConfigInst.PolicyBatchSize,
			Offset:       offset,
		})
		if err != nil {
			return errors.Wrap(err, "dbstore.GetConfigurationPolicies")
		}
		offset += len(policies)

		// Get the set of commits within this repository that match an indexing policy
		commitMap, err := s.policyMatcher.CommitsDescribedByPolicy(ctx, repositoryID, policies, now)
		if err != nil {
			return errors.Wrap(err, "policies.CommitsDescribedByPolicy")
		}

		for commit, policyMatches := range commitMap {
			if len(policyMatches) == 0 {
				continue
			}

			// Attempt to queue an index if one does not exist for each of the matching commits
			if _, err := s.autoindexingSvc.QueueIndexes(ctx, repositoryID, commit, "", false, false); err != nil {
				if errors.HasType(err, &gitdomain.RevisionNotFoundError{}) {
					continue
				}

				return errors.Wrap(err, "indexEnqueuer.QueueIndexes")
			}
		}

		if len(policies) == 0 || offset >= totalCount {
			return nil
		}
	}
}
