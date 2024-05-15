package graphqlbackend

import (
	"context"
	"errors"
	"sync"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater"
	repoupdaterprotocol "github.com/sourcegraph/sourcegraph/pkg/repoupdater/protocol"
)

func (r *repositoryResolver) MirrorInfo() *repositoryMirrorInfoResolver {
	return &repositoryMirrorInfoResolver{repository: r}
}

type repositoryMirrorInfoResolver struct {
	repository *repositoryResolver

	// memoize the gitserver RepoInfo call
	repoInfoOnce     sync.Once
	repoInfoResponse *protocol.RepoInfoResponse
	repoInfoErr      error
}

func (r *repositoryMirrorInfoResolver) gitserverRepoInfo(ctx context.Context) (*protocol.RepoInfoResponse, error) {
	r.repoInfoOnce.Do(func() {
		r.repoInfoResponse, r.repoInfoErr = gitserver.DefaultClient.RepoInfo(ctx, r.repository.repo.URI)
	})
	return r.repoInfoResponse, r.repoInfoErr
}

func (r *repositoryMirrorInfoResolver) RemoteURL(ctx context.Context) (string, error) {
	// 🚨 SECURITY: The remote URL might contain secret credentials in the URL userinfo, so
	// only allow site admins to see it.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return "", err
	}

	{
		// Look up the remote URL in repo-updater.
		result, err := repoupdater.DefaultClient.RepoLookup(ctx, repoupdaterprotocol.RepoLookupArgs{
			Repo:         r.repository.repo.URI,
			ExternalRepo: r.repository.repo.ExternalRepo,
		})
		if err != nil {
			return "", err
		}
		if result.Repo != nil {
			return result.Repo.VCS.URL, nil
		}
	}

	// Fall back to the gitserver repo info for repos on hosts that are not yet fully supported by repo-updater.
	info, err := r.gitserverRepoInfo(ctx)
	if err != nil {
		return "", err
	}
	return info.URL, nil
}

func (r *repositoryMirrorInfoResolver) Cloned(ctx context.Context) (bool, error) {
	info, err := r.gitserverRepoInfo(ctx)
	if err != nil {
		return false, err
	}
	return info.Cloned, nil
}

func (r *repositoryMirrorInfoResolver) CloneInProgress(ctx context.Context) (bool, error) {
	info, err := r.gitserverRepoInfo(ctx)
	if err != nil {
		return false, err
	}
	return info.CloneInProgress, nil
}

func (r *repositoryMirrorInfoResolver) CloneProgress(ctx context.Context) (*string, error) {
	info, err := r.gitserverRepoInfo(ctx)
	if err != nil {
		return nil, err
	}
	return nullString(info.CloneProgress), nil
}

func (r *repositoryMirrorInfoResolver) UpdatedAt(ctx context.Context) (*string, error) {
	info, err := r.gitserverRepoInfo(ctx)
	if err != nil {
		return nil, err
	}
	if info.LastFetched == nil {
		return nil, err
	}
	s := info.LastFetched.Format(time.RFC3339)
	return &s, nil
}

func (r *schemaResolver) CheckMirrorRepositoryConnection(ctx context.Context, args *struct {
	Repository *graphql.ID
	Name       *string
}) (*checkMirrorRepositoryConnectionResult, error) {
	// 🚨 SECURITY: This is an expensive operation and the errors may contain secrets,
	// so only site admins may run it.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	if (args.Repository != nil && args.Name != nil) || (args.Repository == nil && args.Name == nil) {
		return nil, errors.New("exactly one of the repository and name arguments must be set")
	}

	var repo *types.Repo
	switch {
	case args.Repository != nil:
		repoID, err := unmarshalRepositoryID(*args.Repository)
		if err != nil {
			return nil, err
		}
		repo, err = backend.Repos.Get(ctx, repoID)
		if err != nil {
			return nil, err
		}
	case args.Name != nil:
		// GitRepo will use just the URI to look up the repository from repo-updater.
		repo = &types.Repo{URI: api.RepoURI(*args.Name)}
	}

	gitserverRepo, err := backend.GitRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	var result checkMirrorRepositoryConnectionResult
	if err := gitserver.DefaultClient.IsRepoCloneable(ctx, gitserverRepo); err != nil {
		result.errorMessage = err.Error()
	}
	return &result, nil
}

type checkMirrorRepositoryConnectionResult struct {
	errorMessage string
}

func (r *checkMirrorRepositoryConnectionResult) Error() *string {
	if r.errorMessage == "" {
		return nil
	}
	return &r.errorMessage
}

func (r *schemaResolver) UpdateMirrorRepository(ctx context.Context, args *struct {
	Repository graphql.ID
}) (*EmptyResponse, error) {
	// 🚨 SECURITY: There is no reason why non-site-admins would need to run this operation.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	repo, err := repositoryByID(ctx, args.Repository)
	if err != nil {
		return nil, err
	}

	gitserverRepo, err := backend.GitRepo(ctx, repo.repo)
	if err != nil {
		return nil, err
	}
	if err := repoupdater.DefaultClient.EnqueueRepoUpdate(ctx, gitserverRepo); err != nil {
		return nil, err
	}
	return &EmptyResponse{}, nil
}

func (r *schemaResolver) UpdateAllMirrorRepositories(ctx context.Context) (*EmptyResponse, error) {
	// Only usable for self-hosted instances
	if envvar.SourcegraphDotComMode() {
		return nil, errors.New("Not available on sourcegraph.com")
	}
	// 🚨 SECURITY: There is no reason why non-site-admins would need to run this operation.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	reposList, err := db.Repos.List(ctx, db.ReposListOptions{Enabled: true, Disabled: true})
	if err != nil {
		return nil, err
	}

	for _, repo := range reposList {
		gitserverRepo, err := backend.GitRepo(ctx, repo)
		if err != nil {
			return nil, err
		}
		if err := repoupdater.DefaultClient.EnqueueRepoUpdate(ctx, gitserverRepo); err != nil {
			return nil, err
		}
	}
	return &EmptyResponse{}, nil
}
