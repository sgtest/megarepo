package policies

import (
	"context"
	"sort"
	"time"

	policies "github.com/sourcegraph/sourcegraph/internal/codeintel/policies/enterprise"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/policies/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/types"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var _ service = (*Service)(nil)

type service interface {
	// Configurations
	GetConfigurationPolicies(ctx context.Context, opts types.GetConfigurationPoliciesOptions) ([]types.ConfigurationPolicy, int, error)
	GetConfigurationPolicyByID(ctx context.Context, id int) (_ types.ConfigurationPolicy, _ bool, err error)
	CreateConfigurationPolicy(ctx context.Context, configurationPolicy types.ConfigurationPolicy) (types.ConfigurationPolicy, error)
	UpdateConfigurationPolicy(ctx context.Context, policy types.ConfigurationPolicy) (err error)
	DeleteConfigurationPolicyByID(ctx context.Context, id int) (err error)

	// Retention Policy
	GetRetentionPolicyOverview(ctx context.Context, upload types.Upload, matchesOnly bool, first int, after int64, query string, now time.Time) (matches []types.RetentionPolicyMatchCandidate, totalCount int, err error)

	// Repository
	GetPreviewRepositoryFilter(ctx context.Context, patterns []string, limit, offset int) (_ []int, totalCount int, repositoryMatchLimit *int, _ error)
	GetPreviewGitObjectFilter(ctx context.Context, repositoryID int, gitObjectType types.GitObjectType, pattern string) (map[string][]string, error)
	SelectPoliciesForRepositoryMembershipUpdate(ctx context.Context, batchSize int) (configurationPolicies []types.ConfigurationPolicy, err error)
	UpdateReposMatchingPatterns(ctx context.Context, patterns []string, policyID int, repositoryMatchLimit *int) (err error)

	// GetUnsafeDB returns the underlying database handle. This is used by the
	// resolvers that have the old convention of using the database handle directly.
	GetUnsafeDB() database.DB
}

type Service struct {
	store          store.Store
	uploadSvc      UploadService
	gitserver      GitserverClient
	operations     *operations
	matcherMetrics *matcherMetrics
}

func newService(
	policiesStore store.Store,
	uploadSvc UploadService,
	gitserver GitserverClient,
	observationContext *observation.Context,
) *Service {
	return &Service{
		store:          policiesStore,
		uploadSvc:      uploadSvc,
		gitserver:      gitserver,
		operations:     newOperations(observationContext),
		matcherMetrics: newMetrics(observationContext),
	}
}

func (s *Service) getPolicyMatcherFromFactory(gitserver GitserverClient, extractor policies.Extractor, includeTipOfDefaultBranch bool, filterByCreatedDate bool) *policies.Matcher {
	return policies.NewMatcher(gitserver, extractor, includeTipOfDefaultBranch, filterByCreatedDate)
}

func (s *Service) GetConfigurationPolicies(ctx context.Context, opts types.GetConfigurationPoliciesOptions) (_ []types.ConfigurationPolicy, totalCount int, err error) {
	ctx, _, endObservation := s.operations.getConfigurationPolicies.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return s.store.GetConfigurationPolicies(ctx, opts)
}

func (s *Service) GetConfigurationPolicyByID(ctx context.Context, id int) (_ types.ConfigurationPolicy, _ bool, err error) {
	ctx, _, endObservation := s.operations.getConfigurationPoliciesByID.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return s.store.GetConfigurationPolicyByID(ctx, id)
}

func (s *Service) CreateConfigurationPolicy(ctx context.Context, configurationPolicy types.ConfigurationPolicy) (_ types.ConfigurationPolicy, err error) {
	ctx, _, endObservation := s.operations.createConfigurationPolicy.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	policy, err := s.store.CreateConfigurationPolicy(ctx, configurationPolicy)
	if err != nil {
		return policy, err
	}

	if err := s.updateReposMatchingPolicyPatterns(ctx, policy); err != nil {
		return policy, err
	}

	return policy, nil
}

func (s *Service) updateReposMatchingPolicyPatterns(ctx context.Context, policy types.ConfigurationPolicy) error {
	var patterns []string
	if policy.RepositoryPatterns != nil {
		patterns = *policy.RepositoryPatterns
	}

	if len(patterns) == 0 {
		return nil
	}

	var repositoryMatchLimit *int
	if val := conf.CodeIntelAutoIndexingPolicyRepositoryMatchLimit(); val != -1 {
		repositoryMatchLimit = &val
	}

	if err := s.store.UpdateReposMatchingPatterns(ctx, patterns, policy.ID, repositoryMatchLimit); err != nil {
		return err
	}

	return nil
}

func (s *Service) UpdateConfigurationPolicy(ctx context.Context, policy types.ConfigurationPolicy) (err error) {
	ctx, _, endObservation := s.operations.updateConfigurationPolicy.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	if err := s.store.UpdateConfigurationPolicy(ctx, policy); err != nil {
		return err
	}

	return s.updateReposMatchingPolicyPatterns(ctx, policy)
}

func (s *Service) DeleteConfigurationPolicyByID(ctx context.Context, id int) (err error) {
	ctx, _, endObservation := s.operations.deleteConfigurationPolicyByID.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return s.store.DeleteConfigurationPolicyByID(ctx, id)
}

func (s *Service) GetRetentionPolicyOverview(ctx context.Context, upload types.Upload, matchesOnly bool, first int, after int64, query string, now time.Time) (matches []types.RetentionPolicyMatchCandidate, totalCount int, err error) {
	ctx, _, endObservation := s.operations.getRetentionPolicyOverview.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	policyMatcher := s.getPolicyMatcherFromFactory(s.gitserver, policies.RetentionExtractor, true, false)

	configPolicies, _, err := s.GetConfigurationPolicies(ctx, types.GetConfigurationPoliciesOptions{
		RepositoryID:     upload.RepositoryID,
		Term:             query,
		ForDataRetention: true,
		Limit:            first,
		Offset:           int(after),
	})
	if err != nil {
		return nil, 0, err
	}

	visibleCommits, err := s.getCommitsVisibleToUpload(ctx, upload)
	if err != nil {
		return nil, 0, err
	}

	matchingPolicies, err := policyMatcher.CommitsDescribedByPolicyInternal(ctx, upload.RepositoryID, configPolicies, time.Now(), visibleCommits...)
	if err != nil {
		return nil, 0, err
	}

	var (
		potentialMatchIndexSet map[int]int // map of policy ID to array index
		potentialMatches       []types.RetentionPolicyMatchCandidate
	)

	potentialMatches, potentialMatchIndexSet = s.populateMatchingCommits(visibleCommits, upload, matchingPolicies, configPolicies, now)

	if !matchesOnly {
		// populate with remaining unmatched policies
		for _, policy := range configPolicies {
			policy := policy
			if _, ok := potentialMatchIndexSet[policy.ID]; !ok {
				potentialMatches = append(potentialMatches, types.RetentionPolicyMatchCandidate{
					ConfigurationPolicy: &policy,
					Matched:             false,
				})
			}
		}
	}

	sort.Slice(potentialMatches, func(i, j int) bool {
		if potentialMatches[i].ConfigurationPolicy == nil {
			return true
		} else if potentialMatches[j].ConfigurationPolicy == nil {
			return false
		}
		return potentialMatches[i].ID < potentialMatches[j].ID
	})

	return potentialMatches, len(potentialMatches), nil
}

func (s *Service) GetPreviewRepositoryFilter(ctx context.Context, patterns []string, limit, offset int) (_ []int, totalCount int, repositoryMatchLimit *int, err error) {
	ctx, _, endObservation := s.operations.getPreviewRepositoryFilter.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	if val := conf.CodeIntelAutoIndexingPolicyRepositoryMatchLimit(); val != -1 {
		repositoryMatchLimit = &val

		if offset+limit > *repositoryMatchLimit {
			limit = *repositoryMatchLimit - offset
		}
	}

	ids, totalCount, err := s.store.GetRepoIDsByGlobPatterns(ctx, patterns, limit, offset)
	if err != nil {
		return nil, 0, nil, err
	}

	return ids, totalCount, repositoryMatchLimit, nil
}

func (s *Service) GetPreviewGitObjectFilter(ctx context.Context, repositoryID int, gitObjectType types.GitObjectType, pattern string) (_ map[string][]string, err error) {
	ctx, _, endObservation := s.operations.getPreviewGitObjectFilter.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	policyMatcher := s.getPolicyMatcherFromFactory(s.gitserver, policies.NoopExtractor, false, false)
	policyMatches, err := policyMatcher.CommitsDescribedByPolicyInternal(ctx, repositoryID, []types.ConfigurationPolicy{{Type: gitObjectType, Pattern: pattern}}, timeutil.Now())
	if err != nil {
		return nil, err
	}

	namesByCommit := make(map[string][]string, len(policyMatches))
	for commit, policyMatches := range policyMatches {
		names := make([]string, 0, len(policyMatches))
		for _, policyMatch := range policyMatches {
			names = append(names, policyMatch.Name)
		}

		namesByCommit[commit] = names
	}

	return namesByCommit, nil
}

func (s *Service) GetUnsafeDB() database.DB {
	return s.store.GetUnsafeDB()
}

func (s *Service) SelectPoliciesForRepositoryMembershipUpdate(ctx context.Context, batchSize int) (configurationPolicies []types.ConfigurationPolicy, err error) {
	ctx, _, endObservation := s.operations.selectPoliciesForRepositoryMembershipUpdate.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	configurationPolicies, err = s.store.SelectPoliciesForRepositoryMembershipUpdate(ctx, batchSize)
	if err != nil {
		return nil, err
	}

	return configurationPolicies, nil
}

func (s *Service) UpdateReposMatchingPatterns(ctx context.Context, patterns []string, policyID int, repositoryMatchLimit *int) (err error) {
	ctx, _, endObservation := s.operations.updateReposMatchingPatterns.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	return s.store.UpdateReposMatchingPatterns(ctx, patterns, policyID, repositoryMatchLimit)
}

func (s *Service) getCommitsVisibleToUpload(ctx context.Context, upload types.Upload) (commits []string, err error) {
	var token *string
	for first := true; first || token != nil; first = false {
		cs, nextToken, err := s.uploadSvc.GetCommitsVisibleToUpload(ctx, upload.ID, 50, token)
		if err != nil {
			return nil, errors.Wrap(err, "uploadSvc.GetCommitsVisibleToUpload")
		}
		token = nextToken

		commits = append(commits, cs...)
	}

	return
}

// populateMatchingCommits builds a slice of all retention policies that, either directly or via
// a visible upload, apply to the upload. It returns the slice of policies and the set of matching
// policy IDs mapped to their index in the slice.
func (s *Service) populateMatchingCommits(
	visibleCommits []string,
	upload types.Upload,
	matchingPolicies map[string][]policies.PolicyMatch,
	policies []types.ConfigurationPolicy,
	now time.Time,
) ([]types.RetentionPolicyMatchCandidate, map[int]int) {
	var (
		potentialMatches       = make([]types.RetentionPolicyMatchCandidate, 0, len(policies))
		potentialMatchIndexSet = make(map[int]int, len(policies))
	)

	// First add all matches for the commit of this upload. We do this to ensure that if a policy matches both the upload's commit
	// and a visible commit, we ensure an entry for that policy is only added for the upload's commit. This makes the logic in checking
	// the visible commits a bit simpler, as we don't have to check if policy X has already been added for a visible commit in the case
	// that the upload's commit is not first in the list.
	if policyMatches, ok := matchingPolicies[upload.Commit]; ok {
		for _, policyMatch := range policyMatches {
			if policyMatch.PolicyDuration == nil || now.Sub(upload.UploadedAt) < *policyMatch.PolicyDuration {
				policyID := -1
				if policyMatch.PolicyID != nil {
					policyID = *policyMatch.PolicyID
				}
				potentialMatches = append(potentialMatches, types.RetentionPolicyMatchCandidate{
					ConfigurationPolicy: policyByID(policies, policyID),
					Matched:             true,
				})
				potentialMatchIndexSet[policyID] = len(potentialMatches) - 1
			}
		}
	}

	for _, commit := range visibleCommits {
		if commit == upload.Commit {
			continue
		}
		if policyMatches, ok := matchingPolicies[commit]; ok {
			for _, policyMatch := range policyMatches {
				if policyMatch.PolicyDuration == nil || now.Sub(upload.UploadedAt) < *policyMatch.PolicyDuration {
					policyID := -1
					if policyMatch.PolicyID != nil {
						policyID = *policyMatch.PolicyID
					}
					if index, ok := potentialMatchIndexSet[policyID]; ok && potentialMatches[index].ProtectingCommits != nil {
						//  If an entry for the policy already exists and it has > 1 "protecting commits", add this commit too.
						potentialMatches[index].ProtectingCommits = append(potentialMatches[index].ProtectingCommits, commit)
					} else if !ok {
						// Else if there's no entry for the policy, create an entry with this commit as the first "protecting commit".
						// This should never override an entry for a policy matched directly, see the first comment on how this is avoided.
						potentialMatches = append(potentialMatches, types.RetentionPolicyMatchCandidate{
							ConfigurationPolicy: policyByID(policies, policyID),
							Matched:             true,
							ProtectingCommits:   []string{commit},
						})
						potentialMatchIndexSet[policyID] = len(potentialMatches) - 1
					}
				}
			}
		}
	}

	return potentialMatches, potentialMatchIndexSet
}

func policyByID(policies []types.ConfigurationPolicy, id int) *types.ConfigurationPolicy {
	if id == -1 {
		return nil
	}
	for _, policy := range policies {
		if policy.ID == id {
			return &policy
		}
	}
	return nil
}
