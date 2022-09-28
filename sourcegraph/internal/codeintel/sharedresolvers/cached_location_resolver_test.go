package sharedresolvers

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	mockrequire "github.com/derision-test/go-mockgen/testutil/require"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const (
	numRoutines     = 5
	numRepositories = 10
	numCommits      = 10 // per repo
	numPaths        = 10 // per commit
)

func TestCachedLocationResolver(t *testing.T) {
	repos := database.NewStrictMockRepoStore()
	repos.GetFunc.SetDefaultHook(func(v0 context.Context, id api.RepoID) (*types.Repo, error) {
		return &types.Repo{ID: id, CreatedAt: time.Now()}, nil
	})

	db := database.NewStrictMockDB()
	db.ReposFunc.SetDefaultReturn(repos)

	t.Cleanup(func() {
		gitserver.Mocks.ResolveRevision = nil
		backend.Mocks.Repos.GetCommit = nil
	})

	gitserver.Mocks.ResolveRevision = func(spec string, opt gitserver.ResolveRevisionOptions) (api.CommitID, error) {
		return api.CommitID(spec), nil
	}

	var commitCalls uint32
	gitserver.Mocks.GetCommit = func(commitID api.CommitID) (*gitdomain.Commit, error) {
		atomic.AddUint32(&commitCalls, 1)
		return &gitdomain.Commit{ID: commitID}, nil
	}

	cachedResolver := NewCachedLocationResolver(db)

	var repositoryIDs []api.RepoID
	for i := 1; i <= numRepositories; i++ {
		repositoryIDs = append(repositoryIDs, api.RepoID(i))
	}

	var commits []string
	for i := 1; i <= numCommits; i++ {
		commits = append(commits, fmt.Sprintf("%040d", i))
	}

	var paths []string
	for i := 1; i <= numPaths; i++ {
		paths = append(paths, fmt.Sprintf("/foo/%d/bar/baz.go", i))
	}

	type resolverPair struct {
		key      string
		resolver *GitTreeEntryResolver
	}
	resolvers := make(chan resolverPair, numRoutines*len(repositoryIDs)*len(commits)*len(paths))

	var wg sync.WaitGroup
	errs := make(chan error, numRoutines)
	for i := 0; i < numRoutines; i++ {
		wg.Add(1)

		go func() {
			defer wg.Done()

			for _, repositoryID := range repositoryIDs {
				repositoryResolver, err := cachedResolver.Repository(context.Background(), repositoryID)
				if err != nil {
					errs <- err
					return
				}
				repoID, err := UnmarshalRepositoryID(repositoryResolver.ID())
				if err != nil {
					errs <- err
					return
				}
				if repoID != repositoryID {
					errs <- errors.Errorf("unexpected repository id. want=%d have=%d", repositoryID, repoID)
					return
				}
			}

			for _, repositoryID := range repositoryIDs {
				for _, commit := range commits {
					commitResolver, err := cachedResolver.Commit(context.Background(), repositoryID, commit)
					if err != nil {
						errs <- err
						return
					}
					if commitResolver.OID() != GitObjectID(commit) {
						errs <- errors.Errorf("unexpected commit. want=%s have=%s", commit, commitResolver.OID())
						return
					}
				}
			}

			for _, repositoryID := range repositoryIDs {
				for _, commit := range commits {
					for _, path := range paths {
						treeResolver, err := cachedResolver.Path(context.Background(), repositoryID, commit, path)
						if err != nil {
							errs <- err
							return
						}
						if treeResolver.Path() != path {
							errs <- errors.Errorf("unexpected path. want=%s have=%s", path, treeResolver.Path())
							return
						}

						resolvers <- resolverPair{key: fmt.Sprintf("%d:%s:%s", repositoryID, commit, path), resolver: treeResolver}
					}
				}
			}
		}()
	}
	wg.Wait()

	close(errs)
	for err := range errs {
		t.Error(err)
	}

	mockrequire.CalledN(t, repos.GetFunc, len(repositoryIDs))

	// We don't need to load commits from git-server unless we ask for fields like author or committer.
	// Since we already know this commit exists, and we only need it's already known commit ID, we assert
	// that zero calls to git.GetCommit where done. Check the gitCommitResolver lazy loading logic.
	if val := atomic.LoadUint32(&commitCalls); val != 0 {
		t.Errorf("unexpected number of commit calls. want=%d have=%d", 0, val)
	}

	close(resolvers)
	resolversByKey := map[string][]*GitTreeEntryResolver{}
	for pair := range resolvers {
		resolversByKey[pair.key] = append(resolversByKey[pair.key], pair.resolver)
	}

	for _, vs := range resolversByKey {
		for _, v := range vs {
			if v != vs[0] {
				t.Errorf("resolvers for same key unexpectedly have differing addresses: %p and %p", v, vs[0])
			}
		}
	}
}

func TestCachedLocationResolverUnknownRepository(t *testing.T) {
	repos := database.NewStrictMockRepoStore()
	repos.GetFunc.SetDefaultHook(func(_ context.Context, id api.RepoID) (*types.Repo, error) {
		return nil, &database.RepoNotFoundErr{ID: id}
	})

	db := database.NewStrictMockDB()
	db.ReposFunc.SetDefaultReturn(repos)

	repositoryResolver, err := NewCachedLocationResolver(db).Repository(context.Background(), 50)
	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}
	if repositoryResolver != nil {
		t.Errorf("unexpected non-nil resolver")
	}

	// Ensure no dereference in child resolvers either
	pathResolver, err := NewCachedLocationResolver(db).Path(context.Background(), 50, "deadbeef", "main.go")
	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}
	if pathResolver != nil {
		t.Errorf("unexpected non-nil resolver")
	}
	mockrequire.Called(t, repos.GetFunc)
}

func TestCachedLocationResolverUnknownCommit(t *testing.T) {
	repos := database.NewStrictMockRepoStore()
	repos.GetFunc.SetDefaultHook(func(_ context.Context, id api.RepoID) (*types.Repo, error) {
		return &types.Repo{ID: id}, nil
	})

	db := database.NewStrictMockDB()
	db.ReposFunc.SetDefaultReturn(repos)

	gitserver.Mocks.ResolveRevision = func(spec string, opt gitserver.ResolveRevisionOptions) (api.CommitID, error) {
		return "", &gitdomain.RevisionNotFoundError{}
	}
	t.Cleanup(func() { gitserver.Mocks.ResolveRevision = nil })

	commitResolver, err := NewCachedLocationResolver(db).Commit(context.Background(), 50, "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}
	if commitResolver != nil {
		t.Errorf("unexpected non-nil resolver")
	}

	// Ensure no dereference in child resolvers either
	pathResolver, err := NewCachedLocationResolver(db).Path(context.Background(), 50, "deadbeef", "main.go")
	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}
	if pathResolver != nil {
		t.Errorf("unexpected non-nil resolver")
	}
	mockrequire.Called(t, repos.GetFunc)
}
