package database

import (
	"context"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestIterateRepoGitserverStatus(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := dbtest.NewDB(t, "")
	ctx := context.Background()

	repo1 := &types.Repo{
		Name:         "github.com/sourcegraph/repo1",
		URI:          "github.com/sourcegraph/repo1",
		Description:  "",
		ExternalRepo: api.ExternalRepoSpec{},
		Sources:      nil,
	}
	repo2 := &types.Repo{
		Name:         "github.com/sourcegraph/repo2",
		URI:          "github.com/sourcegraph/repo2",
		Description:  "",
		ExternalRepo: api.ExternalRepoSpec{},
		Sources:      nil,
	}

	// Create two test repos
	err := Repos(db).Create(ctx, repo1, repo2)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo := &types.GitserverRepo{
		RepoID:              repo1.ID,
		ShardID:             "gitserver1",
		CloneStatus:         types.CloneStatusNotCloned,
		LastExternalService: 0,
	}

	// Create one GitServerRepo
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}

	var repoCount int
	var statusCount int

	// Iterate
	err = GitserverRepos(db).IterateRepoGitserverStatus(ctx, func(repo types.RepoGitserverStatus) error {
		repoCount++
		if repo.GitserverRepo != nil {
			statusCount++
			if repo.GitserverRepo.RepoID == 0 {
				t.Fatal("GitServerRepo has zero id")
			}
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}

	wantRepoCount := 2
	if repoCount != wantRepoCount {
		t.Fatalf("Expected %d repos, got %d", wantRepoCount, repoCount)
	}

	wantStatusCount := 1
	if statusCount != wantStatusCount {
		t.Fatalf("Expected %d statuses, got %d", wantStatusCount, statusCount)
	}
}

func TestGitserverReposGetByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := dbtest.NewDB(t, "")
	ctx := context.Background()

	_, err := GitserverRepos(db).GetByID(ctx, 1)
	if err == nil {
		t.Fatal("Expected an error")
	}

	repo1 := &types.Repo{
		Name:         "github.com/sourcegraph/repo1",
		URI:          "github.com/sourcegraph/repo1",
		Description:  "",
		ExternalRepo: api.ExternalRepoSpec{},
		Sources:      nil,
	}

	// Create one test repo
	err = Repos(db).Create(ctx, repo1)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo := &types.GitserverRepo{
		RepoID:              repo1.ID,
		ShardID:             "test",
		CloneStatus:         types.CloneStatusNotCloned,
		LastExternalService: 0,
	}

	// Create GitServerRepo
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}

	// GetByID should now work
	fromDB, err := GitserverRepos(db).GetByID(ctx, gitserverRepo.RepoID)
	if err != nil {
		t.Fatal(err)
	}

	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}
}

func TestSetCloneStatus(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := dbtest.NewDB(t, "")
	ctx := context.Background()
	const shardID = "test"

	repo1 := &types.Repo{
		Name:         "github.com/sourcegraph/repo1",
		URI:          "github.com/sourcegraph/repo1",
		ExternalRepo: api.ExternalRepoSpec{},
	}

	// Create one test repo
	err := Repos(db).Create(ctx, repo1)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo := &types.GitserverRepo{
		RepoID:      repo1.ID,
		ShardID:     shardID,
		CloneStatus: types.CloneStatusNotCloned,
	}

	// Create GitServerRepo
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}

	// Get it back
	fromDB, err := GitserverRepos(db).GetByID(ctx, gitserverRepo.RepoID)
	if err != nil {
		t.Fatal(err)
	}

	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Set cloned
	err = GitserverRepos(db).SetCloneStatus(ctx, gitserverRepo.RepoID, types.CloneStatusCloned, shardID)
	if err != nil {
		t.Fatal(err)
	}

	fromDB, err = GitserverRepos(db).GetByID(ctx, gitserverRepo.RepoID)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo.CloneStatus = types.CloneStatusCloned
	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Setting clone status should work even if no row exists
	repo2 := &types.Repo{
		Name:         "github.com/sourcegraph/repo2",
		URI:          "github.com/sourcegraph/repo2",
		ExternalRepo: api.ExternalRepoSpec{},
	}

	// Create one test repo
	err = Repos(db).Create(ctx, repo2)
	if err != nil {
		t.Fatal(err)
	}

	if err := GitserverRepos(db).SetCloneStatus(ctx, repo2.ID, types.CloneStatusCloned, shardID); err != nil {
		t.Fatal(err)
	}
	fromDB, err = GitserverRepos(db).GetByID(ctx, repo2.ID)
	if err != nil {
		t.Fatal(err)
	}
	gitserverRepo2 := &types.GitserverRepo{
		RepoID:      repo2.ID,
		ShardID:     shardID,
		CloneStatus: types.CloneStatusCloned,
	}
	if diff := cmp.Diff(gitserverRepo2, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Setting the same status again should not touch the row
	if err := GitserverRepos(db).SetCloneStatus(ctx, repo2.ID, types.CloneStatusCloned, shardID); err != nil {
		t.Fatal(err)
	}
	after, err := GitserverRepos(db).GetByID(ctx, repo2.ID)
	if err != nil {
		t.Fatal(err)
	}
	if diff := cmp.Diff(fromDB, after); diff != "" {
		t.Fatal(diff)
	}
}

func TestSetLastError(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := dbtest.NewDB(t, "")
	ctx := context.Background()
	const shardID = "test"

	repo1 := &types.Repo{
		Name:         "github.com/sourcegraph/repo1",
		URI:          "github.com/sourcegraph/repo1",
		ExternalRepo: api.ExternalRepoSpec{},
	}

	// Create one test repo
	err := Repos(db).Create(ctx, repo1)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo := &types.GitserverRepo{
		RepoID:      repo1.ID,
		ShardID:     shardID,
		CloneStatus: types.CloneStatusNotCloned,
	}

	// Create GitServerRepo
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}

	// Set error
	err = GitserverRepos(db).SetLastError(ctx, gitserverRepo.RepoID, "oops", shardID)
	if err != nil {
		t.Fatal(err)
	}

	fromDB, err := GitserverRepos(db).GetByID(ctx, gitserverRepo.RepoID)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo.LastError = "oops"
	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Remove error
	err = GitserverRepos(db).SetLastError(ctx, gitserverRepo.RepoID, "", shardID)
	if err != nil {
		t.Fatal(err)
	}

	fromDB, err = GitserverRepos(db).GetByID(ctx, gitserverRepo.RepoID)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo.LastError = ""
	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Set again to same value, updated_at should not change
	err = GitserverRepos(db).SetLastError(ctx, gitserverRepo.RepoID, "", shardID)
	if err != nil {
		t.Fatal(err)
	}

	after, err := GitserverRepos(db).GetByID(ctx, gitserverRepo.RepoID)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo.LastError = ""
	if diff := cmp.Diff(fromDB, after); diff != "" {
		t.Fatal(diff)
	}

	// Setting to empty error should set the column to null
	count, _, err := basestore.ScanFirstInt(db.QueryContext(ctx, "SELECT COUNT(*) FROM gitserver_repos WHERE last_error IS NULL"))
	if err != nil {
		t.Fatal(err)
	}

	if count != 1 {
		t.Fatalf("Want %d, got %d", 1, count)
	}
}

func TestGitserverRepoUpsertNullShard(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := dbtest.NewDB(t, "")
	ctx := context.Background()

	repo1 := &types.Repo{
		Name:         "github.com/sourcegraph/repo1",
		URI:          "github.com/sourcegraph/repo1",
		Description:  "",
		ExternalRepo: api.ExternalRepoSpec{},
		Sources:      nil,
	}

	// Create one test repo
	err := Repos(db).Create(ctx, repo1)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo := &types.GitserverRepo{
		RepoID:              repo1.ID,
		ShardID:             "",
		CloneStatus:         types.CloneStatusNotCloned,
		LastExternalService: 0,
	}

	// Create one GitServerRepo
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err == nil {
		t.Fatal("Expected an error")
	}
}

func TestGitserverRepoUpsert(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := dbtest.NewDB(t, "")
	ctx := context.Background()

	repo1 := &types.Repo{
		Name:         "github.com/sourcegraph/repo1",
		URI:          "github.com/sourcegraph/repo1",
		Description:  "",
		ExternalRepo: api.ExternalRepoSpec{},
		Sources:      nil,
	}

	// Create two test repos
	err := Repos(db).Create(ctx, repo1)
	if err != nil {
		t.Fatal(err)
	}

	gitserverRepo := &types.GitserverRepo{
		RepoID:      repo1.ID,
		ShardID:     "abc",
		CloneStatus: types.CloneStatusNotCloned,
	}

	// Create one GitServerRepo
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}

	// Get it back from the db
	fromDB, err := GitserverRepos(db).GetByID(ctx, repo1.ID)
	if err != nil {
		t.Fatal(err)
	}

	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Change clone status
	gitserverRepo.CloneStatus = types.CloneStatusCloned
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}
	fromDB, err = GitserverRepos(db).GetByID(ctx, repo1.ID)
	if err != nil {
		t.Fatal(err)
	}
	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Change error
	gitserverRepo.LastError = "Oops"
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}
	fromDB, err = GitserverRepos(db).GetByID(ctx, repo1.ID)
	if err != nil {
		t.Fatal(err)
	}
	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Remove error
	gitserverRepo.LastError = ""
	if err := GitserverRepos(db).Upsert(ctx, gitserverRepo); err != nil {
		t.Fatal(err)
	}
	fromDB, err = GitserverRepos(db).GetByID(ctx, repo1.ID)
	if err != nil {
		t.Fatal(err)
	}
	if diff := cmp.Diff(gitserverRepo, fromDB, cmpopts.IgnoreFields(types.GitserverRepo{}, "UpdatedAt")); diff != "" {
		t.Fatal(diff)
	}
}
