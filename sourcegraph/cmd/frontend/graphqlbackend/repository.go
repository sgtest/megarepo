package graphqlbackend

import (
	"bytes"
	"context"
	"fmt"
	"sync"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/externallink"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/phabricator"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/vcs"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

type repositoryResolver struct {
	hydration sync.Once
	err       error

	repo        *types.Repo
	redirectURL *string
	icon        string
	matches     []*searchResultMatchResolver
}

func repositoryByID(ctx context.Context, id graphql.ID) (*repositoryResolver, error) {
	var repoID api.RepoID
	if err := relay.UnmarshalSpec(id, &repoID); err != nil {
		return nil, err
	}
	repo, err := db.Repos.Get(ctx, repoID)
	if err != nil {
		return nil, err
	}
	return &repositoryResolver{repo: repo}, nil
}

func repositoryByIDInt32(ctx context.Context, repoID api.RepoID) (*repositoryResolver, error) {
	repo, err := db.Repos.Get(ctx, repoID)
	if err != nil {
		return nil, err
	}
	return &repositoryResolver{repo: repo}, nil
}

func (r *repositoryResolver) ID() graphql.ID {
	return marshalRepositoryID(r.repo.ID)
}

func marshalRepositoryID(repo api.RepoID) graphql.ID { return relay.MarshalID("Repository", repo) }

func unmarshalRepositoryID(id graphql.ID) (repo api.RepoID, err error) {
	err = relay.UnmarshalSpec(id, &repo)
	return
}

func (r *repositoryResolver) Name() string {
	return string(r.repo.Name)
}

func (r *repositoryResolver) URI(ctx context.Context) (string, error) {
	err := r.hydrate(ctx)
	if err != nil {
		return "", err
	}

	return r.repo.URI, nil
}

func (r *repositoryResolver) Description(ctx context.Context) (string, error) {
	err := r.hydrate(ctx)
	if err != nil {
		return "", err
	}

	return r.repo.Description, nil
}

func (r *repositoryResolver) RedirectURL() *string {
	return r.redirectURL
}

func (r *repositoryResolver) ViewerCanAdminister(ctx context.Context) (bool, error) {
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		if err == backend.ErrMustBeSiteAdmin || err == backend.ErrNotAuthenticated {
			return false, nil // not an error
		}
		return false, err
	}
	return true, nil
}

func (r *repositoryResolver) CloneInProgress(ctx context.Context) (bool, error) {
	return r.MirrorInfo().CloneInProgress(ctx)
}

type repositoryCommitArgs struct {
	Rev          string
	InputRevspec *string
}

func (r *repositoryResolver) Commit(ctx context.Context, args *repositoryCommitArgs) (*gitCommitResolver, error) {
	commitID, err := backend.Repos.ResolveRev(ctx, r.repo, args.Rev)
	if err != nil {
		if gitserver.IsRevisionNotFound(err) {
			return nil, nil
		}
		return nil, err
	}

	commit, err := backend.Repos.GetCommit(ctx, r.repo, commitID)
	if commit == nil || err != nil {
		return nil, err
	}

	resolver := toGitCommitResolver(r, commit)
	if args.InputRevspec != nil {
		resolver.inputRev = args.InputRevspec
	} else {
		resolver.inputRev = &args.Rev
	}
	return resolver, nil
}

func (r *repositoryResolver) DefaultBranch(ctx context.Context) (*gitRefResolver, error) {
	cachedRepo, err := backend.CachedGitRepo(ctx, r.repo)
	if err != nil {
		return nil, err
	}

	refBytes, _, exitCode, err := git.ExecSafe(ctx, *cachedRepo, []string{"symbolic-ref", "HEAD"})
	refName := string(bytes.TrimSpace(refBytes))

	if err == nil && exitCode == 0 {
		// Check that our repo is not empty
		_, err = git.ResolveRevision(ctx, *cachedRepo, nil, "HEAD", &git.ResolveRevisionOptions{NoEnsureRevision: true})
	}

	// If we fail to get the default branch due to cloning or being empty, we return nothing.
	if err != nil {
		if vcs.IsCloneInProgress(err) || gitserver.IsRevisionNotFound(err) {
			return nil, nil
		}
		return nil, err
	}

	return &gitRefResolver{repo: r, name: refName}, nil
}

func (r *repositoryResolver) Language(ctx context.Context) string {
	// The repository language is the most common language at the HEAD commit of the repository.
	// Note: the repository database field is no longer updated as of
	// https://github.com/sourcegraph/sourcegraph/issues/2586, so we do not use it anymore and
	// instead compute the language on the fly.

	commitID, err := backend.Repos.ResolveRev(ctx, r.repo, "")
	if err != nil {
		return ""
	}

	inventory, err := backend.Repos.GetInventory(ctx, r.repo, commitID)
	if err != nil {
		return ""
	}
	if len(inventory.Languages) == 0 {
		return ""
	}
	return inventory.Languages[0].Name
}

func (r *repositoryResolver) Enabled() bool { return true }

func (r *repositoryResolver) CreatedAt() string {
	return time.Now().Format(time.RFC3339)
}

func (r *repositoryResolver) UpdatedAt() *string {
	return nil
}

func (r *repositoryResolver) URL() string { return "/" + string(r.repo.Name) }

func (r *repositoryResolver) ExternalURLs(ctx context.Context) ([]*externallink.Resolver, error) {
	return externallink.Repository(ctx, r.repo)
}

func (r *repositoryResolver) Icon() string {
	return r.icon
}

func (r *repositoryResolver) Label() (*markdownResolver, error) {
	text := "[" + string(r.repo.Name) + "](/" + string(r.repo.Name) + ")"
	return &markdownResolver{text: text}, nil
}

func (r *repositoryResolver) Detail() *markdownResolver {
	return &markdownResolver{text: "Repository name match"}
}

func (r *repositoryResolver) Matches() []*searchResultMatchResolver {
	return r.matches
}

func (r *repositoryResolver) ToRepository() (*repositoryResolver, bool) { return r, true }
func (r *repositoryResolver) ToFileMatch() (*fileMatchResolver, bool)   { return nil, false }
func (r *repositoryResolver) ToCommitSearchResult() (*commitSearchResultResolver, bool) {
	return nil, false
}
func (r *repositoryResolver) ToCodemodResult() (*codemodResultResolver, bool) {
	return nil, false
}

func (r *repositoryResolver) searchResultURIs() (string, string) {
	return string(r.repo.Name), ""
}

func (r *repositoryResolver) resultCount() int32 {
	return 1
}

func (r *repositoryResolver) hydrate(ctx context.Context) error {
	r.hydration.Do(func() {
		if r.repo.RepoFields != nil {
			return
		}

		log15.Debug("repositoryResolver.hydrate", "repo.ID", r.repo.ID)

		var repo *types.Repo
		repo, r.err = db.Repos.Get(ctx, r.repo.ID)
		if r.err == nil {
			r.repo.RepoFields = repo.RepoFields
		}
	})

	return r.err
}

func (*schemaResolver) AddPhabricatorRepo(ctx context.Context, args *struct {
	Callsign string
	Name     *string
	// TODO(chris): Remove URI in favor of Name.
	URI *string
	URL string
}) (*EmptyResponse, error) {
	if args.Name != nil {
		args.URI = args.Name
	}

	_, err := db.Phabricator.CreateIfNotExists(ctx, args.Callsign, api.RepoName(*args.URI), args.URL)
	if err != nil {
		log15.Error("adding phabricator repo", "callsign", args.Callsign, "name", args.URI, "url", args.URL)
	}
	return nil, err
}

func (*schemaResolver) ResolvePhabricatorDiff(ctx context.Context, args *struct {
	RepoName    string
	DiffID      int32
	BaseRev     string
	Patch       *string
	AuthorName  *string
	AuthorEmail *string
	Description *string
	Date        *string
}) (*gitCommitResolver, error) {
	repo, err := db.Repos.GetByName(ctx, api.RepoName(args.RepoName))
	if err != nil {
		return nil, err
	}
	targetRef := fmt.Sprintf("phabricator/diff/%d", args.DiffID)
	getCommit := func() (*gitCommitResolver, error) {
		// We first check via the vcsrepo api so that we can toggle
		// NoEnsureRevision. We do this, otherwise repositoryResolver.Commit
		// will try and fetch it from the remote host. However, this is not on
		// the remote host since we created it.
		cachedRepo, err := backend.CachedGitRepo(ctx, repo)
		if err != nil {
			return nil, err
		}
		_, err = git.ResolveRevision(ctx, *cachedRepo, nil, targetRef, &git.ResolveRevisionOptions{
			NoEnsureRevision: true,
		})
		if err != nil {
			return nil, err
		}
		r := &repositoryResolver{repo: repo}
		return r.Commit(ctx, &repositoryCommitArgs{Rev: targetRef})
	}

	// If we already created the commit
	if commit, err := getCommit(); commit != nil || (err != nil && !gitserver.IsRevisionNotFound(err)) {
		return commit, err
	}

	origin := ""
	if phabRepo, err := db.Phabricator.GetByName(ctx, api.RepoName(args.RepoName)); err == nil {
		origin = phabRepo.URL
	}

	if origin == "" {
		return nil, errors.New("unable to resolve the origin of the phabricator instance")
	}

	client, clientErr := makePhabClientForOrigin(ctx, origin)

	patch := ""
	if args.Patch != nil {
		patch = *args.Patch
	} else if client == nil {
		return nil, clientErr
	} else {
		diff, err := client.GetRawDiff(ctx, int(args.DiffID))
		// No diff contents were given and we couldn't fetch them
		if err != nil {
			return nil, err
		}

		patch = diff
	}

	var info *phabricator.DiffInfo
	if client != nil && (args.AuthorEmail == nil || args.AuthorName == nil || args.Date == nil) {
		info, err = client.GetDiffInfo(ctx, int(args.DiffID))
		// Not all the information was given and we couldn't fetch it
		if err != nil {
			return nil, err
		}
	} else {
		var description, authorName, authorEmail string
		if args.Description != nil {
			description = *args.Description
		}
		if args.AuthorName != nil {
			authorName = *args.AuthorName
		}
		if args.AuthorEmail != nil {
			authorEmail = *args.AuthorEmail
		}
		date, err := phabricator.ParseDate(*args.Date)
		if err != nil {
			return nil, err
		}

		info = &phabricator.DiffInfo{
			AuthorName:  authorName,
			AuthorEmail: authorEmail,
			Message:     description,
			Date:        *date,
		}
	}

	_, err = gitserver.DefaultClient.CreateCommitFromPatch(ctx, protocol.CreateCommitFromPatchRequest{
		Repo:       api.RepoName(args.RepoName),
		BaseCommit: api.CommitID(args.BaseRev),
		TargetRef:  targetRef,
		Patch:      patch,
		CommitInfo: protocol.PatchCommitInfo{
			AuthorName:  info.AuthorName,
			AuthorEmail: info.AuthorEmail,
			Message:     info.Message,
			Date:        info.Date,
		},
	})
	if err != nil {
		return nil, err
	}

	return getCommit()
}

func makePhabClientForOrigin(ctx context.Context, origin string) (*phabricator.Client, error) {
	phabs, err := db.ExternalServices.ListPhabricatorConnections(ctx)
	if err != nil {
		return nil, err
	}

	for _, phab := range phabs {
		if phab.Url != origin {
			continue
		}

		if phab.Token == "" {
			return nil, errors.Errorf("no phabricator token was given for: %s", origin)
		}

		return phabricator.NewClient(ctx, phab.Url, phab.Token, nil)
	}

	return nil, errors.Errorf("no phabricator was configured for: %s", origin)
}
