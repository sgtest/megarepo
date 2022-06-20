package handlerutil

import (
	"context"
	"testing"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestGetRepo(t *testing.T) {
	logger := logtest.Scoped(t)
	t.Run("URLMovedError", func(t *testing.T) {
		backend.Mocks.Repos.GetByName = func(ctx context.Context, name api.RepoName) (*types.Repo, error) {
			return &types.Repo{Name: name + name}, nil
		}
		t.Cleanup(func() {
			backend.Mocks.Repos = backend.MockRepos{}
		})

		_, err := GetRepo(context.Background(), logger, database.NewMockDB(), map[string]string{"Repo": "repo1"})
		if !errors.HasType(err, &URLMovedError{}) {
			t.Fatalf("err: want type *URLMovedError but got %T", err)
		}
	})
}
