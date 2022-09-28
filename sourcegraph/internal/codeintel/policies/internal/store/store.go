package store

import (
	"context"

	logger "github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/types"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// Store provides the interface for policies storage.
type Store interface {
	// Configurations
	GetConfigurationPolicies(ctx context.Context, opts types.GetConfigurationPoliciesOptions) (_ []types.ConfigurationPolicy, totalCount int, err error)
	GetConfigurationPolicyByID(ctx context.Context, id int) (_ types.ConfigurationPolicy, _ bool, err error)
	CreateConfigurationPolicy(ctx context.Context, configurationPolicy types.ConfigurationPolicy) (types.ConfigurationPolicy, error)
	UpdateConfigurationPolicy(ctx context.Context, policy types.ConfigurationPolicy) (err error)
	DeleteConfigurationPolicyByID(ctx context.Context, id int) (err error)

	// Repositories
	GetRepoIDsByGlobPatterns(ctx context.Context, patterns []string, limit, offset int) (_ []int, _ int, err error)
	UpdateReposMatchingPatterns(ctx context.Context, patterns []string, policyID int, repositoryMatchLimit *int) (err error)

	// Utilities
	GetUnsafeDB() database.DB
	SelectPoliciesForRepositoryMembershipUpdate(ctx context.Context, batchSize int) (configurationPolicies []types.ConfigurationPolicy, err error)
}

// store manages the policies store.
type store struct {
	db         *basestore.Store
	logger     logger.Logger
	operations *operations
}

// New returns a new policies store.
func New(db database.DB, observationContext *observation.Context) Store {
	return &store{
		db:         basestore.NewWithHandle(db.Handle()),
		logger:     logger.Scoped("policies.store", ""),
		operations: newOperations(observationContext),
	}
}

// GetUnsafeDB returns the underlying database handle. This is used by the
// resolvers that have the old convention of using the database handle directly.
func (s *store) GetUnsafeDB() database.DB {
	return database.NewDBWith(s.logger, s.db)
}
