package dependencies

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/internal/lockfiles"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/internal/store"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
)

type Store interface {
	ListDependencyRepos(ctx context.Context, opts store.ListDependencyReposOpts) ([]Repo, error)
	UpsertDependencyRepos(ctx context.Context, deps []Repo) ([]Repo, error)
	DeleteDependencyReposByID(ctx context.Context, ids ...int) error
}

type LockfilesService interface {
	ListDependencies(ctx context.Context, repo api.RepoName, rev string) ([]reposource.PackageDependency, error)
}

type GitService = lockfiles.GitService

type Syncer interface {
	// Sync will lazily sync the repos that have been inserted into the database but have not yet been
	// cloned. See repos.Syncer.SyncRepo.
	Sync(ctx context.Context, repo api.RepoName) error
}
