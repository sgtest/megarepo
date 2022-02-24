package dbstore_test

import (
	"context"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
)

func TestRepoName(t *testing.T) {
	db := dbtest.NewDB(t)
	store := testStore(db)

	if _, err := db.Exec(`INSERT INTO repo (id, name) VALUES (50, 'github.com/foo/bar')`); err != nil {
		t.Fatalf("unexpected error inserting repo: %s", err)
	}

	name, err := store.RepoName(context.Background(), 50)
	if err != nil {
		t.Fatalf("unexpected error getting repo name: %s", err)
	}
	if name != "github.com/foo/bar" {
		t.Errorf("unexpected repo name. want=%s have=%s", "github.com/foo/bar", name)
	}
}

func TestUpsertDependencyRepo(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := database.NewDB(dbtest.NewDB(t))
	store := testStore(db)

	for _, dep := range []reposource.PackageDependency{
		parseNPMDependency(t, "bar@2.0.0"),
		parseNPMDependency(t, "bar@2.0.0"),
		parseNPMDependency(t, "bar@3.0.0"),
		parseNPMDependency(t, "foo@1.0.0"),
		parseNPMDependency(t, "foo@1.0.0"),
		parseNPMDependency(t, "foo@2.0.0"),
	} {
		if err := store.UpsertDependencyRepo(ctx, dep); err != nil {
			t.Fatal(err)
		}
	}

	have, err := store.GetNPMDependencyRepos(ctx, dbstore.GetNPMDependencyReposOpts{})
	if err != nil {
		t.Fatal(err)
	}

	want := []dbstore.NPMDependencyRepo{
		{Package: "foo", Version: "2.0.0"},
		{Package: "foo", Version: "1.0.0"},
		{Package: "bar", Version: "3.0.0"},
		{Package: "bar", Version: "2.0.0"},
	}

	opt := cmpopts.IgnoreFields(dbstore.NPMDependencyRepo{}, "ID")
	if diff := cmp.Diff(have, want, opt); diff != "" {
		t.Fatalf("mismatch (-have, +want): %s", diff)
	}
}

func parseNPMDependency(t testing.TB, dep string) reposource.PackageDependency {
	t.Helper()

	d, err := reposource.ParseNPMDependency(dep)
	if err != nil {
		t.Fatal(err)
	}

	return d
}
