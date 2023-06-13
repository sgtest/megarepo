package graphqlbackend

import (
	"context"
	"io/fs"
	"net/url"
	"os"
	"sync"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/externallink"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (r *schemaResolver) gitCommitByID(ctx context.Context, id graphql.ID) (*GitCommitResolver, error) {
	repoID, commitID, err := unmarshalGitCommitID(id)
	if err != nil {
		return nil, err
	}
	repo, err := r.repositoryByID(ctx, repoID)
	if err != nil {
		return nil, err
	}
	return repo.Commit(ctx, &RepositoryCommitArgs{Rev: string(commitID)})
}

// GitCommitResolver resolves git commits.
//
// Prefer using NewGitCommitResolver to create an instance of the commit resolver.
type GitCommitResolver struct {
	logger          log.Logger
	db              database.DB
	gitserverClient gitserver.Client
	repoResolver    *RepositoryResolver

	// inputRev is the Git revspec that the user originally requested that resolved to this Git commit. It is used
	// to avoid redirecting a user browsing a revision "mybranch" to the absolute commit ID as they follow links in the UI.
	inputRev *string

	// fetch + serve sourcegraph stored user information
	includeUserInfo bool

	// oid MUST be specified and a 40-character Git SHA.
	oid GitObjectID

	gitRepo api.RepoName

	// commit should not be accessed directly since it might not be initialized.
	// Use the resolver methods instead.
	commit     *gitdomain.Commit
	commitOnce sync.Once
	commitErr  error
}

// NewGitCommitResolver returns a new CommitResolver. When commit is set to nil,
// commit will be loaded lazily as needed by the resolver. Pass in a commit when
// you have batch-loaded a bunch of them and already have them at hand.
func NewGitCommitResolver(db database.DB, gsClient gitserver.Client, repo *RepositoryResolver, id api.CommitID, commit *gitdomain.Commit) *GitCommitResolver {
	repoName := repo.RepoName()
	return &GitCommitResolver{
		logger: log.Scoped("gitCommitResolver", "resolve a specific commit").With(
			log.String("repo", string(repoName)),
			log.String("commitID", string(id)),
		),
		db:              db,
		gitserverClient: gsClient,
		repoResolver:    repo,
		includeUserInfo: true,
		gitRepo:         repoName,
		oid:             GitObjectID(id),
		commit:          commit,
	}
}

func (r *GitCommitResolver) resolveCommit(ctx context.Context) (*gitdomain.Commit, error) {
	r.commitOnce.Do(func() {
		if r.commit != nil {
			return
		}

		opts := gitserver.ResolveRevisionOptions{}
		r.commit, r.commitErr = r.gitserverClient.GetCommit(ctx, authz.DefaultSubRepoPermsChecker, r.gitRepo, api.CommitID(r.oid), opts)
	})
	return r.commit, r.commitErr
}

// gitCommitGQLID is a type used for marshaling and unmarshalling a Git commit's
// GraphQL ID.
type gitCommitGQLID struct {
	Repository graphql.ID  `json:"r"`
	CommitID   GitObjectID `json:"c"`
}

func marshalGitCommitID(repo graphql.ID, commitID GitObjectID) graphql.ID {
	return relay.MarshalID("GitCommit", gitCommitGQLID{Repository: repo, CommitID: commitID})
}

func unmarshalGitCommitID(id graphql.ID) (repoID graphql.ID, commitID GitObjectID, err error) {
	var spec gitCommitGQLID
	err = relay.UnmarshalSpec(id, &spec)
	return spec.Repository, spec.CommitID, err
}

func (r *GitCommitResolver) ID() graphql.ID {
	return marshalGitCommitID(r.repoResolver.ID(), r.oid)
}

func (r *GitCommitResolver) Repository() *RepositoryResolver { return r.repoResolver }

func (r *GitCommitResolver) OID() GitObjectID { return r.oid }

func (r *GitCommitResolver) InputRev() *string { return r.inputRev }

func (r *GitCommitResolver) AbbreviatedOID() string {
	return string(r.oid)[:7]
}

func (r *GitCommitResolver) PerforceChangelist(ctx context.Context) (*PerforceChangelistResolver, error) {
	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return nil, err
	}

	return toPerforceChangelistResolver(ctx, r.repoResolver, commit)
}

func (r *GitCommitResolver) Author(ctx context.Context) (*signatureResolver, error) {
	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return nil, err
	}
	return toSignatureResolver(r.db, &commit.Author, r.includeUserInfo), nil
}

func (r *GitCommitResolver) Committer(ctx context.Context) (*signatureResolver, error) {
	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return nil, err
	}
	return toSignatureResolver(r.db, commit.Committer, r.includeUserInfo), nil
}

func (r *GitCommitResolver) Message(ctx context.Context) (string, error) {
	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return "", err
	}
	return string(commit.Message), err
}

func (r *GitCommitResolver) Subject(ctx context.Context) (string, error) {
	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return "", err
	}

	if subject := maybeTransformP4Subject(ctx, r.repoResolver, commit); subject != nil {
		return *subject, nil
	}

	return commit.Message.Subject(), nil
}

func (r *GitCommitResolver) Body(ctx context.Context) (*string, error) {
	if r.repoResolver.isPerforceDepot(ctx) {
		return nil, nil
	}

	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return nil, err
	}

	body := commit.Message.Body()
	if body == "" {
		return nil, nil
	}

	return &body, nil
}

func (r *GitCommitResolver) Parents(ctx context.Context) ([]*GitCommitResolver, error) {
	commit, err := r.resolveCommit(ctx)
	if err != nil {
		return nil, err
	}

	resolvers := make([]*GitCommitResolver, len(commit.Parents))
	// TODO(tsenart): We can get the parent commits in batch from gitserver instead of doing
	// N roundtrips. We already have a git.Commits method. Maybe we can use that.
	for i, parent := range commit.Parents {
		var err error
		resolvers[i], err = r.repoResolver.Commit(ctx, &RepositoryCommitArgs{Rev: string(parent)})
		if err != nil {
			return nil, err
		}
	}
	return resolvers, nil
}

func (r *GitCommitResolver) URL() string {
	repoUrl := r.repoResolver.url()
	repoUrl.Path += "/-/commit/" + r.inputRevOrImmutableRev()
	return repoUrl.String()
}

func (r *GitCommitResolver) CanonicalURL() string {
	repoUrl := r.repoResolver.url()
	repoUrl.Path += "/-/commit/" + string(r.oid)
	return repoUrl.String()
}

func (r *GitCommitResolver) ExternalURLs(ctx context.Context) ([]*externallink.Resolver, error) {
	repo, err := r.repoResolver.repo(ctx)
	if err != nil {
		return nil, err
	}

	return externallink.Commit(ctx, r.db, repo, api.CommitID(r.oid))
}

func (r *GitCommitResolver) Tree(ctx context.Context, args *struct {
	Path      string
	Recursive bool
}) (*GitTreeEntryResolver, error) {
	treeEntry, err := r.path(ctx, args.Path, func(stat fs.FileInfo) error {
		if !stat.Mode().IsDir() {
			return errors.Errorf("not a directory: %q", args.Path)
		}

		return nil
	})
	if err != nil {
		return nil, err
	}

	// Note: args.Recursive is deprecated
	if treeEntry != nil {
		treeEntry.isRecursive = args.Recursive
	}
	return treeEntry, nil
}

func (r *GitCommitResolver) Blob(ctx context.Context, args *struct {
	Path string
}) (*GitTreeEntryResolver, error) {
	return r.path(ctx, args.Path, func(stat fs.FileInfo) error {
		if mode := stat.Mode(); !(mode.IsRegular() || mode.Type()&fs.ModeSymlink != 0) {
			return errors.Errorf("not a blob: %q", args.Path)
		}

		return nil
	})
}

func (r *GitCommitResolver) File(ctx context.Context, args *struct {
	Path string
}) (*GitTreeEntryResolver, error) {
	return r.Blob(ctx, args)
}

func (r *GitCommitResolver) Path(ctx context.Context, args *struct {
	Path string
}) (*GitTreeEntryResolver, error) {
	return r.path(ctx, args.Path, func(_ fs.FileInfo) error { return nil })
}

func (r *GitCommitResolver) path(ctx context.Context, path string, validate func(fs.FileInfo) error) (_ *GitTreeEntryResolver, err error) {
	tr, ctx := trace.New(ctx, "GitCommitResolver", "path",
		attribute.String("path", path))
	defer tr.FinishWithErr(&err)

	stat, err := r.gitserverClient.Stat(ctx, authz.DefaultSubRepoPermsChecker, r.gitRepo, api.CommitID(r.oid), path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	if err := validate(stat); err != nil {
		return nil, err
	}
	opts := GitTreeEntryResolverOpts{
		Commit: r,
		Stat:   stat,
	}
	return NewGitTreeEntryResolver(r.db, r.gitserverClient, opts), nil
}

func (r *GitCommitResolver) FileNames(ctx context.Context) ([]string, error) {
	return r.gitserverClient.LsFiles(ctx, authz.DefaultSubRepoPermsChecker, r.gitRepo, api.CommitID(r.oid))
}

func (r *GitCommitResolver) Languages(ctx context.Context) ([]string, error) {
	repo, err := r.repoResolver.repo(ctx)
	if err != nil {
		return nil, err
	}

	inventory, err := backend.NewRepos(r.logger, r.db, r.gitserverClient).GetInventory(ctx, repo, api.CommitID(r.oid), false)
	if err != nil {
		return nil, err
	}

	names := make([]string, len(inventory.Languages))
	for i, l := range inventory.Languages {
		names[i] = l.Name
	}
	return names, nil
}

func (r *GitCommitResolver) LanguageStatistics(ctx context.Context) ([]*languageStatisticsResolver, error) {
	repo, err := r.repoResolver.repo(ctx)
	if err != nil {
		return nil, err
	}

	inventory, err := backend.NewRepos(r.logger, r.db, r.gitserverClient).GetInventory(ctx, repo, api.CommitID(r.oid), false)
	if err != nil {
		return nil, err
	}
	stats := make([]*languageStatisticsResolver, 0, len(inventory.Languages))
	for _, lang := range inventory.Languages {
		stats = append(stats, &languageStatisticsResolver{
			l: lang,
		})
	}
	return stats, nil
}

type AncestorsArgs struct {
	graphqlutil.ConnectionArgs
	Query       *string
	Path        *string
	Follow      bool
	After       *string
	AfterCursor *string
	Before      *string
}

func (r *GitCommitResolver) Ancestors(ctx context.Context, args *AncestorsArgs) (*gitCommitConnectionResolver, error) {
	return &gitCommitConnectionResolver{
		db:              r.db,
		gitserverClient: r.gitserverClient,
		revisionRange:   string(r.oid),
		first:           args.ConnectionArgs.First,
		query:           args.Query,
		path:            args.Path,
		follow:          args.Follow,
		after:           args.After,
		afterCursor:     args.AfterCursor,
		before:          args.Before,
		repo:            r.repoResolver,
	}, nil
}

func (r *GitCommitResolver) Diff(ctx context.Context, args *struct {
	Base *string
}) (*RepositoryComparisonResolver, error) {
	oidString := string(r.oid)
	base := oidString + "~"
	if args.Base != nil {
		base = *args.Base
	}
	return NewRepositoryComparison(ctx, r.db, r.gitserverClient, r.repoResolver, &RepositoryComparisonInput{
		Base:         &base,
		Head:         &oidString,
		FetchMissing: false,
	})
}

func (r *GitCommitResolver) BehindAhead(ctx context.Context, args *struct {
	Revspec string
}) (*behindAheadCountsResolver, error) {
	counts, err := r.gitserverClient.GetBehindAhead(ctx, r.gitRepo, args.Revspec, string(r.oid))
	if err != nil {
		return nil, err
	}

	return &behindAheadCountsResolver{
		behind: int32(counts.Behind),
		ahead:  int32(counts.Ahead),
	}, nil
}

type behindAheadCountsResolver struct{ behind, ahead int32 }

func (r *behindAheadCountsResolver) Behind() int32 { return r.behind }
func (r *behindAheadCountsResolver) Ahead() int32  { return r.ahead }

// inputRevOrImmutableRev returns the input revspec, if it is provided and nonempty. Otherwise it returns the
// canonical OID for the revision.
func (r *GitCommitResolver) inputRevOrImmutableRev() string {
	if r.inputRev != nil && *r.inputRev != "" {
		return *r.inputRev
	}
	return string(r.oid)
}

// repoRevURL returns the URL path prefix to use when constructing URLs to resources at this
// revision. Unlike inputRevOrImmutableRev, it does NOT use the OID if no input revspec is
// given. This is because the convention in the frontend is for repo-rev URLs to omit the "@rev"
// portion (unlike for commit page URLs, which must include some revspec in
// "/REPO/-/commit/REVSPEC").
func (r *GitCommitResolver) repoRevURL() *url.URL {
	// Dereference to copy to avoid mutation
	repoUrl := *r.repoResolver.RepoMatch.URL()
	var rev string
	if r.inputRev != nil {
		rev = *r.inputRev // use the original input rev from the user
	} else {
		rev = string(r.oid)
	}
	if rev != "" {
		repoUrl.Path += "@" + rev
	}
	return &repoUrl
}

func (r *GitCommitResolver) canonicalRepoRevURL() *url.URL {
	// Dereference to copy the URL to avoid mutation
	repoUrl := *r.repoResolver.RepoMatch.URL()
	repoUrl.Path += "@" + string(r.oid)
	return &repoUrl
}

func (r *GitCommitResolver) Ownership(ctx context.Context, args ListOwnershipArgs) (OwnershipConnectionResolver, error) {
	return EnterpriseResolvers.ownResolver.GitCommitOwnership(ctx, r, args)
}
