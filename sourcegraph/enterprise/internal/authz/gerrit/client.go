package gerrit

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/extsvc/gerrit"
)

type client interface {
	ListAccountsByEmail(ctx context.Context, email string) (gerrit.ListAccountsResponse, error)
	ListAccountsByUsername(ctx context.Context, username string) (gerrit.ListAccountsResponse, error)
	GetGroup(ctx context.Context, groupName string) (gerrit.Group, error)
}

var _ client = (*ClientAdapter)(nil)

// ClientAdapter is an adapter for Gerrit API client.
type ClientAdapter struct {
	*gerrit.Client
}

type mockClient struct {
	mockListAccountsByEmail    func(ctx context.Context, email string) (gerrit.ListAccountsResponse, error)
	mockListAccountsByUsername func(ctx context.Context, username string) (gerrit.ListAccountsResponse, error)
	mockGetGroup               func(ctx context.Context, groupName string) (gerrit.Group, error)
}

func (m *mockClient) ListAccountsByEmail(ctx context.Context, email string) (gerrit.ListAccountsResponse, error) {
	if m.mockListAccountsByEmail != nil {
		return m.mockListAccountsByEmail(ctx, email)
	}
	return nil, nil
}

func (m *mockClient) ListAccountsByUsername(ctx context.Context, username string) (gerrit.ListAccountsResponse, error) {
	if m.mockListAccountsByUsername != nil {
		return m.mockListAccountsByUsername(ctx, username)
	}
	return nil, nil
}

func (m *mockClient) GetGroup(ctx context.Context, groupName string) (gerrit.Group, error) {
	if m.mockGetGroup != nil {
		return m.mockGetGroup(ctx, groupName)
	}
	return gerrit.Group{}, nil
}
