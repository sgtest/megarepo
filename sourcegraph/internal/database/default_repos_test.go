package database

import (
	"context"
	"sort"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestListDefaultRepos(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	tcs := []struct {
		name  string
		repos []*types.RepoName
	}{
		{
			name:  "empty case",
			repos: nil,
		},
		{
			name: "one repo",
			repos: []*types.RepoName{
				{
					ID:   api.RepoID(1),
					Name: "github.com/foo/bar",
				},
			},
		},
		{
			name: "a few repos",
			repos: []*types.RepoName{
				{
					ID:   api.RepoID(1),
					Name: "github.com/foo/bar",
				},
				{
					ID:   api.RepoID(2),
					Name: "github.com/baz/qux",
				},
			},
		},
	}

	for _, tc := range tcs {
		t.Run(tc.name, func(t *testing.T) {
			db := dbtesting.GetDB(t)
			ctx := context.Background()
			for _, r := range tc.repos {
				if _, err := db.ExecContext(ctx, `INSERT INTO repo(id, name) VALUES ($1, $2)`, r.ID, r.Name); err != nil {
					t.Fatal(err)
				}
				if _, err := db.ExecContext(ctx, `INSERT INTO default_repos(repo_id) VALUES ($1)`, r.ID); err != nil {
					t.Fatal(err)
				}
			}
			DefaultRepos(db).resetCache()

			repos, err := DefaultRepos(db).List(ctx)
			if err != nil {
				t.Fatal(err)
			}

			sort.Sort(types.RepoNames(repos))
			sort.Sort(types.RepoNames(tc.repos))
			if diff := cmp.Diff(repos, tc.repos, cmpopts.EquateEmpty()); diff != "" {
				t.Errorf("mismatch (-want +got):\n%s", diff)
			}
		})
	}

	t.Run("user-added repos", func(t *testing.T) {
		db := dbtesting.GetDB(t)
		ctx := context.Background()
		_, err := db.ExecContext(ctx, `
			-- insert one user-added repo, i.e. a repo added by an external service owned by a user
			INSERT INTO users(id, username) VALUES (1, 'foo');
			INSERT INTO repo(id, name) VALUES (10, 'github.com/foo/bar10');
			INSERT INTO external_services(id, kind, display_name, config, namespace_user_id) VALUES (100, 'github', 'github', '{}', 1);
			INSERT INTO external_service_repos VALUES (100, 10, 'https://github.com/foo/bar10');

			-- insert one repo referenced in the default repo table
			INSERT INTO repo(id, name) VALUES (11, 'github.com/foo/bar11');
			INSERT INTO default_repos(repo_id) VALUES(11);

			-- insert one repo not referenced in the default repo table;
			INSERT INTO repo(id, name) VALUES (12, 'github.com/foo/bar12');

			-- insert a repo only references by a cloud_default external service
			INSERT INTO repo(id, name) VALUES (13, 'github.com/foo/bar13');
			INSERT INTO external_services(id, kind, display_name, config, cloud_default) VALUES (101, 'github', 'github', '{}', true);
			INSERT INTO external_service_repos VALUES (101, 13, 'https://github.com/foo/bar13');
		`)
		if err != nil {
			t.Fatal(err)
		}

		DefaultRepos(db).resetCache()

		repos, err := DefaultRepos(db).List(ctx)
		if err != nil {
			t.Fatal(err)
		}

		want := []*types.RepoName{
			{
				ID:   api.RepoID(10),
				Name: "github.com/foo/bar10",
			},
			{
				ID:   api.RepoID(11),
				Name: "github.com/foo/bar11",
			},
		}
		// expect 2 repos, the user added repo and the one that is referenced in the default repos table
		if diff := cmp.Diff(want, repos, cmpopts.EquateEmpty()); diff != "" {
			t.Errorf("mismatch (-want +got):\n%s", diff)
		}
	})
}

func TestListDefaultReposInBatches(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	reposToAdd := []*types.RepoName{
		{
			ID:   api.RepoID(1),
			Name: "github.com/foo/bar1",
		},
		{
			ID:   api.RepoID(2),
			Name: "github.com/baz/bar2",
		},
		{
			ID:   api.RepoID(3),
			Name: "github.com/foo/bar3",
		},
	}

	db := dbtesting.GetDB(t)
	ctx := context.Background()
	for _, r := range reposToAdd {
		if _, err := db.ExecContext(ctx, `INSERT INTO repo(id, name) VALUES ($1, $2)`, r.ID, r.Name); err != nil {
			t.Fatal(err)
		}
		if _, err := db.ExecContext(ctx, `INSERT INTO default_repos(repo_id) VALUES ($1)`, r.ID); err != nil {
			t.Fatal(err)
		}
	}

	repos, err := Repos(db).listAllDefaultRepos(ctx, 2)
	if err != nil {
		t.Fatal(err)
	}

	sort.Sort(types.RepoNames(repos))
	sort.Sort(types.RepoNames(reposToAdd))
	if diff := cmp.Diff(repos, reposToAdd, cmpopts.EquateEmpty()); diff != "" {
		t.Errorf("mismatch (-want +got):\n%s", diff)
	}
}

func TestListDefaultReposUncloned(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	reposToAdd := []*types.RepoName{
		{
			ID:   api.RepoID(1),
			Name: "github.com/foo/bar1",
		},
		{
			ID:   api.RepoID(2),
			Name: "github.com/baz/bar2",
		},
		{
			ID:   api.RepoID(3),
			Name: "github.com/foo/bar3",
		},
	}

	db := dbtesting.GetDB(t)
	ctx := context.Background()
	for _, r := range reposToAdd {
		cloned := int(r.ID) > 1
		if _, err := db.ExecContext(ctx, `INSERT INTO repo(id, name, cloned) VALUES ($1, $2, $3)`, r.ID, r.Name, cloned); err != nil {
			t.Fatal(err)
		}
		if _, err := db.ExecContext(ctx, `INSERT INTO default_repos(repo_id) VALUES ($1)`, r.ID); err != nil {
			t.Fatal(err)
		}
	}

	repos, err := Repos(db).ListDefaultRepos(ctx, ListDefaultReposOptions{
		Limit:        3,
		AfterID:      0,
		OnlyUncloned: true,
	})
	if err != nil {
		t.Fatal(err)
	}

	sort.Sort(types.RepoNames(repos))
	sort.Sort(types.RepoNames(reposToAdd))
	if diff := cmp.Diff(repos, reposToAdd[:1], cmpopts.EquateEmpty()); diff != "" {
		t.Errorf("mismatch (-want +got):\n%s", diff)
	}
}

func BenchmarkDefaultRepos_List_Empty(b *testing.B) {
	db := dbtest.NewDB(b, "")

	ctx := context.Background()
	select {
	case <-ctx.Done():
		b.Fatal("context already canceled")
	default:
	}
	b.ResetTimer()
	for n := 0; n < b.N; n++ {
		_, err := DefaultRepos(db).List(ctx)
		if err != nil {
			b.Fatal(err)
		}
	}
}
