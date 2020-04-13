package github

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
)

// client defines the set of GitHub API client methods used by the authz provider.
//
// NOTE: All methods are sorted in alphabetical order.
type client interface {
	GetRepositoryByNodeID(ctx context.Context, token, id string) (*github.Repository, error)
	GetRepositoriesByNodeIDFromAPI(ctx context.Context, token string, nodeIDs []string) (map[string]*github.Repository, error)
	ListAffiliatedRepositories(ctx context.Context, page int) (repos []*github.Repository, hasNextPage bool, rateLimitCost int, err error)
	ListRepositoryCollaborators(ctx context.Context, owner, repo string, page int) (users []*github.Collaborator, hasNextPage bool, _ error)
	WithToken(token string) client
}

var _ client = (*clientAdapter)(nil)

// clientAdapter is an adapter for GitHub API client.
type clientAdapter struct {
	*github.Client
}

func (c *clientAdapter) WithToken(token string) client {
	return &clientAdapter{Client: c.Client.WithToken(token)}
}

var _ client = (*mockClient)(nil)

type mockClient struct {
	MockGetRepositoryByNodeID          func(ctx context.Context, token, id string) (*github.Repository, error)
	MockGetRepositoriesByNodeIDFromAPI func(ctx context.Context, token string, nodeIDs []string) (map[string]*github.Repository, error)
	MockListAffiliatedRepositories     func(ctx context.Context, page int) (repos []*github.Repository, hasNextPage bool, rateLimitCost int, err error)
	MockListRepositoryCollaborators    func(ctx context.Context, owner, repo string, page int) (users []*github.Collaborator, hasNextPage bool, _ error)
	MockWithToken                      func(token string) client
}

func (m *mockClient) GetRepositoryByNodeID(ctx context.Context, token, id string) (*github.Repository, error) {
	return m.MockGetRepositoryByNodeID(ctx, token, id)
}

func (m *mockClient) GetRepositoriesByNodeIDFromAPI(ctx context.Context, token string, nodeIDs []string) (map[string]*github.Repository, error) {
	return m.MockGetRepositoriesByNodeIDFromAPI(ctx, token, nodeIDs)
}

func (m *mockClient) ListAffiliatedRepositories(ctx context.Context, page int) ([]*github.Repository, bool, int, error) {
	return m.MockListAffiliatedRepositories(ctx, page)
}

func (m *mockClient) ListRepositoryCollaborators(ctx context.Context, owner, repo string, page int) ([]*github.Collaborator, bool, error) {
	return m.MockListRepositoryCollaborators(ctx, owner, repo, page)
}

func (m *mockClient) WithToken(token string) client {
	return m.MockWithToken(token)
}
