package store

import (
	"context"
	"fmt"
	"sort"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestSetRepositoryAsDirty(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	for _, id := range []int{50, 51, 52} {
		insertRepo(t, db, id, "", false)
	}

	for _, repositoryID := range []int{50, 51, 52, 51, 52} {
		if err := store.SetRepositoryAsDirty(context.Background(), repositoryID); err != nil {
			t.Errorf("unexpected error marking repository as dirty: %s", err)
		}
	}

	dirtyRepositories, err := store.GetDirtyRepositories(context.Background())
	if err != nil {
		t.Fatalf("unexpected error listing dirty repositories: %s", err)
	}

	var keys []int
	for _, dirtyRepository := range dirtyRepositories {
		keys = append(keys, dirtyRepository.RepositoryID)
	}
	sort.Ints(keys)

	if diff := cmp.Diff([]int{50, 51, 52}, keys); diff != "" {
		t.Errorf("unexpected repository ids (-want +got):\n%s", diff)
	}
}

func TestGetRepositoriesMaxStaleAge(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	for _, id := range []int{50, 51, 52} {
		insertRepo(t, db, id, "", false)
	}

	if _, err := db.ExecContext(context.Background(), `
		INSERT INTO lsif_dirty_repositories (
			repository_id,
			update_token,
			dirty_token,
			set_dirty_at
		)
		VALUES
			(50, 10, 10, NOW() - '45 minutes'::interval), -- not dirty
			(51, 20, 25, NOW() - '30 minutes'::interval), -- dirty
			(52, 30, 35, NOW() - '20 minutes'::interval), -- dirty
			(53, 40, 45, NOW() - '30 minutes'::interval); -- no associated repo
	`); err != nil {
		t.Fatalf("unexpected error marking repostiory as dirty: %s", err)
	}

	age, err := store.GetRepositoriesMaxStaleAge(context.Background())
	if err != nil {
		t.Fatalf("unexpected error listing dirty repositories: %s", err)
	}
	if age.Round(time.Second) != 30*time.Minute {
		t.Fatalf("unexpected max age. want=%s have=%s", 30*time.Minute, age)
	}
}

func TestHasRepository(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	testCases := []struct {
		repositoryID int
		exists       bool
	}{
		{50, true},
		{51, false},
		{52, false},
	}

	insertUploads(t, db, shared.Upload{ID: 1, RepositoryID: 50})
	insertUploads(t, db, shared.Upload{ID: 2, RepositoryID: 51, State: "deleted"})

	for _, testCase := range testCases {
		name := fmt.Sprintf("repositoryID=%d", testCase.repositoryID)

		t.Run(name, func(t *testing.T) {
			exists, err := store.HasRepository(context.Background(), testCase.repositoryID)
			if err != nil {
				t.Fatalf("unexpected error checking if repository exists: %s", err)
			}
			if exists != testCase.exists {
				t.Errorf("unexpected exists. want=%v have=%v", testCase.exists, exists)
			}
		})
	}
}

func TestSkipsDeletedRepositories(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	insertRepo(t, db, 50, "should not be dirty", false)
	deleteRepo(t, db, 50, time.Now())

	insertRepo(t, db, 51, "should be dirty", false)

	// NOTE: We did not insert 52, so it should not show up as dirty, even though we mark it below.

	for _, repositoryID := range []int{50, 51, 52} {
		if err := store.SetRepositoryAsDirty(context.Background(), repositoryID); err != nil {
			t.Fatalf("unexpected error marking repository as dirty: %s", err)
		}
	}

	dirtyRepositories, err := store.GetDirtyRepositories(context.Background())
	if err != nil {
		t.Fatalf("unexpected error listing dirty repositories: %s", err)
	}

	var keys []int
	for _, dirtyRepository := range dirtyRepositories {
		keys = append(keys, dirtyRepository.RepositoryID)
	}
	sort.Ints(keys)

	if diff := cmp.Diff([]int{51}, keys); diff != "" {
		t.Errorf("unexpected repository ids (-want +got):\n%s", diff)
	}
}

// Marks a repo as deleted
func deleteRepo(t testing.TB, db database.DB, id int, deleted_at time.Time) {
	query := sqlf.Sprintf(
		`UPDATE repo SET deleted_at = %s WHERE id = %s`,
		deleted_at,
		id,
	)
	if _, err := db.ExecContext(context.Background(), query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
		t.Fatalf("unexpected error while deleting repository: %s", err)
	}
}

func testStoreWithoutConfigurationPolicies(t *testing.T, db database.DB) Store {
	if _, err := db.ExecContext(context.Background(), `TRUNCATE lsif_configuration_policies`); err != nil {
		t.Fatalf("unexpected error while inserting configuration policies: %s", err)
	}

	store := New(&observation.TestContext, db)
	return store
}

func TestNumRepositoriesWithCodeIntelligence(t *testing.T) {
	ctx := context.Background()
	logger := logtest.Scoped(t)
	sqlDB := dbtest.NewDB(logger, t)
	db := database.NewDB(logger, sqlDB)
	store := New(&observation.TestContext, db)

	insertUploads(t, db,
		shared.Upload{ID: 100, RepositoryID: 50},
		shared.Upload{ID: 101, RepositoryID: 51},
		shared.Upload{ID: 102, RepositoryID: 52}, // Not in commit graph
		shared.Upload{ID: 103, RepositoryID: 53}, // Not on default branch
	)

	if _, err := db.ExecContext(ctx, `
		INSERT INTO lsif_uploads_visible_at_tip
			(repository_id, upload_id, is_default_branch)
		VALUES
			(50, 100, true),
			(51, 101, true),
			(53, 103, false)
	`); err != nil {
		t.Fatalf("unexpected error inserting visible uploads: %s", err)
	}

	count, err := store.NumRepositoriesWithCodeIntelligence(ctx)
	if err != nil {
		t.Fatalf("unexpected error getting top repositories to configure: %s", err)
	}
	if expected := 2; count != expected {
		t.Fatalf("unexpected number of repositories. want=%d have=%d", expected, count)
	}
}

func TestRepositoryIDsWithErrors(t *testing.T) {
	ctx := context.Background()
	logger := logtest.Scoped(t)
	sqlDB := dbtest.NewDB(logger, t)
	db := database.NewDB(logger, sqlDB)
	store := New(&observation.TestContext, db)

	now := time.Now()
	t1 := now.Add(-time.Minute * 1)
	t2 := now.Add(-time.Minute * 2)
	t3 := now.Add(-time.Minute * 3)

	insertUploads(t, db,
		shared.Upload{ID: 100, RepositoryID: 50},                  // Repo 50 = success (no index)
		shared.Upload{ID: 101, RepositoryID: 51},                  // Repo 51 = success (+ successful index)
		shared.Upload{ID: 103, RepositoryID: 53, State: "failed"}, // Repo 53 = failed

		// Repo 54 = multiple failures for same project
		shared.Upload{ID: 150, RepositoryID: 54, State: "failed", FinishedAt: &t1},
		shared.Upload{ID: 151, RepositoryID: 54, State: "failed", FinishedAt: &t2},
		shared.Upload{ID: 152, RepositoryID: 54, State: "failed", FinishedAt: &t3},

		// Repo 55 = multiple failures for different projects
		shared.Upload{ID: 160, RepositoryID: 55, State: "failed", FinishedAt: &t1, Root: "proj1"},
		shared.Upload{ID: 161, RepositoryID: 55, State: "failed", FinishedAt: &t2, Root: "proj2"},
		shared.Upload{ID: 162, RepositoryID: 55, State: "failed", FinishedAt: &t3, Root: "proj3"},

		// Repo 58 = multiple failures with later success (not counted)
		shared.Upload{ID: 170, RepositoryID: 58, State: "completed", FinishedAt: &t1},
		shared.Upload{ID: 171, RepositoryID: 58, State: "failed", FinishedAt: &t2},
		shared.Upload{ID: 172, RepositoryID: 58, State: "failed", FinishedAt: &t3},
	)
	insertIndexes(t, db,
		uploadsshared.Index{ID: 201, RepositoryID: 51},                  // Repo 51 = success
		uploadsshared.Index{ID: 202, RepositoryID: 52, State: "failed"}, // Repo 52 = failing index
		uploadsshared.Index{ID: 203, RepositoryID: 53},                  // Repo 53 = success (+ failing upload)

		// Repo 56 = multiple failures for same project
		uploadsshared.Index{ID: 250, RepositoryID: 56, State: "failed", FinishedAt: &t1},
		uploadsshared.Index{ID: 251, RepositoryID: 56, State: "failed", FinishedAt: &t2},
		uploadsshared.Index{ID: 252, RepositoryID: 56, State: "failed", FinishedAt: &t3},

		// Repo 57 = multiple failures for different projects
		uploadsshared.Index{ID: 260, RepositoryID: 57, State: "failed", FinishedAt: &t1, Root: "proj1"},
		uploadsshared.Index{ID: 261, RepositoryID: 57, State: "failed", FinishedAt: &t2, Root: "proj2"},
		uploadsshared.Index{ID: 262, RepositoryID: 57, State: "failed", FinishedAt: &t3, Root: "proj3"},
	)

	// Query page 1
	repositoriesWithCount, totalCount, err := store.RepositoryIDsWithErrors(ctx, 0, 4)
	if err != nil {
		t.Fatalf("unexpected error getting repositories with errors: %s", err)
	}
	if expected := 6; totalCount != expected {
		t.Fatalf("unexpected total number of repositories. want=%d have=%d", expected, totalCount)
	}
	expected := []uploadsshared.RepositoryWithCount{
		{RepositoryID: 55, Count: 3},
		{RepositoryID: 57, Count: 3},
		{RepositoryID: 52, Count: 1},
		{RepositoryID: 53, Count: 1},
	}
	if diff := cmp.Diff(expected, repositoriesWithCount); diff != "" {
		t.Errorf("unexpected repositories (-want +got):\n%s", diff)
	}

	// Query page 2
	repositoriesWithCount, _, err = store.RepositoryIDsWithErrors(ctx, 4, 4)
	if err != nil {
		t.Fatalf("unexpected error getting repositories with errors: %s", err)
	}
	expected = []uploadsshared.RepositoryWithCount{
		{RepositoryID: 54, Count: 1},
		{RepositoryID: 56, Count: 1},
	}
	if diff := cmp.Diff(expected, repositoriesWithCount); diff != "" {
		t.Errorf("unexpected repositories (-want +got):\n%s", diff)
	}
}
