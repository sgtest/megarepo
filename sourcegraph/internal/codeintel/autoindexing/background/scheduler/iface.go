package scheduler

import (
	"context"
	"time"

	policies "github.com/sourcegraph/sourcegraph/internal/codeintel/policies/enterprise"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/policies/shared"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
)

type DBStore interface {
	GetConfigurationPolicies(ctx context.Context, opts dbstore.GetConfigurationPoliciesOptions) ([]dbstore.ConfigurationPolicy, int, error)
	SelectRepositoriesForIndexScan(ctx context.Context, table, column string, processDelay time.Duration, allowGlobalPolicies bool, repositoryMatchLimit *int, limit int) ([]int, error)
}

type PolicyMatcher interface {
	CommitsDescribedByPolicy(ctx context.Context, repositoryID int, policies []dbstore.ConfigurationPolicy, now time.Time, filterCommits ...string) (map[string][]policies.PolicyMatch, error)
}

type PolicyService interface {
	GetConfigurationPolicies(ctx context.Context, opts shared.GetConfigurationPoliciesOptions) ([]shared.ConfigurationPolicy, int, error)
}
