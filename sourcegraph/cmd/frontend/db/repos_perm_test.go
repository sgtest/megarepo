package db

import (
	"context"
	"testing"

	"github.com/keegancsmith/sqlf"
	dbtesting "github.com/sourcegraph/sourcegraph/cmd/frontend/db/testing"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
)

// 🚨 SECURITY: test necessary to ensure security
func Test_getBySQL_permissionsCheck(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	defer func() { mockAuthzFilter = nil }()

	ctx := dbtesting.TestContext(t)

	allRepos := mustCreate(ctx, t, &types.Repo{
		URI: "r0",
		ExternalRepo: &api.ExternalRepoSpec{
			ID:          "a0",
			ServiceType: "b0",
			ServiceID:   "c0",
		},
	}, &types.Repo{
		URI: "r1",
		ExternalRepo: &api.ExternalRepoSpec{
			ID:          "a1",
			ServiceType: "b1",
			ServiceID:   "c1",
		},
	})
	{
		calledFilter := false
		mockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perm) ([]*types.Repo, error) {
			calledFilter = true
			return repos, nil
		}

		gotRepos, err := Repos.getBySQL(ctx, sqlf.Sprintf("WHERE true"))
		if err != nil {
			t.Fatal(err)
		}
		if !jsonEqual(t, gotRepos, allRepos) {
			t.Errorf("got %v, want %v", gotRepos, allRepos)
		}
		if !calledFilter {
			t.Error("did not call authzFilter (SECURITY)")
		}
	}
	{
		calledFilter := false
		mockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perm) ([]*types.Repo, error) {
			calledFilter = true
			return nil, nil
		}

		gotRepos, err := Repos.getBySQL(ctx, sqlf.Sprintf("WHERE true"))
		if err != nil {
			t.Fatal(err)
		}
		if !jsonEqual(t, gotRepos, nil) {
			t.Errorf("got %v, want %v", gotRepos, nil)
		}
		if !calledFilter {
			t.Error("did not call authzFilter (SECURITY)")
		}
	}
	{
		calledFilter := false
		filteredRepos := allRepos[0:1]
		mockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perm) ([]*types.Repo, error) {
			calledFilter = true
			return filteredRepos, nil
		}

		gotRepos, err := Repos.getBySQL(ctx, sqlf.Sprintf("WHERE true"))
		if err != nil {
			t.Fatal(err)
		}
		if !jsonEqual(t, gotRepos, filteredRepos) {
			t.Errorf("got %v, want %v", gotRepos, filteredRepos)
		}
		if !calledFilter {
			t.Error("did not call authzFilter (SECURITY)")
		}
	}
}
