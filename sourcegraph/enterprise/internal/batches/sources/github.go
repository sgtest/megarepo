package sources

import (
	"context"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

type GithubSource struct {
	client *github.V4Client
	au     auth.Authenticator
}

var _ ForkableChangesetSource = GithubSource{}

func NewGithubSource(ctx context.Context, svc *types.ExternalService, cf *httpcli.Factory) (*GithubSource, error) {
	rawConfig, err := svc.Config.Decrypt(ctx)
	if err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	var c schema.GitHubConnection
	if err := jsonc.Unmarshal(rawConfig, &c); err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newGithubSource(svc.URN(), &c, cf, nil)
}

func newGithubSource(urn string, c *schema.GitHubConnection, cf *httpcli.Factory, au auth.Authenticator) (*GithubSource, error) {
	baseURL, err := url.Parse(c.Url)
	if err != nil {
		return nil, err
	}
	baseURL = extsvc.NormalizeBaseURL(baseURL)

	apiURL, _ := github.APIRoot(baseURL)

	if cf == nil {
		cf = httpcli.ExternalClientFactory
	}

	opts := httpClientCertificateOptions([]httpcli.Opt{
		// Use a 30s timeout to avoid running into EOF errors, because GitHub
		// closes idle connections after 60s
		httpcli.NewIdleConnTimeoutOpt(30 * time.Second),
	}, c.Certificate)

	cli, err := cf.Doer(opts...)
	if err != nil {
		return nil, err
	}

	var authr = au
	if au == nil {
		authr = &auth.OAuthBearerToken{Token: c.Token}
	}

	return &GithubSource{
		au:     authr,
		client: github.NewV4Client(urn, apiURL, authr, cli),
	}, nil
}

func (s GithubSource) GitserverPushConfig(repo *types.Repo) (*protocol.PushConfig, error) {
	return GitserverPushConfig(repo, s.au)
}

func (s GithubSource) WithAuthenticator(a auth.Authenticator) (ChangesetSource, error) {
	switch a.(type) {
	case *auth.OAuthBearerToken,
		*auth.OAuthBearerTokenWithSSH:
		break

	default:
		return nil, newUnsupportedAuthenticatorError("GithubSource", a)
	}

	sc := s
	sc.au = a
	sc.client = sc.client.WithAuthenticator(a)

	return &sc, nil
}

func (s GithubSource) ValidateAuthenticator(ctx context.Context) error {
	_, err := s.client.GetAuthenticatedUser(ctx)
	return err
}

// CreateChangeset creates the given changeset on the code host.
func (s GithubSource) CreateChangeset(ctx context.Context, c *Changeset) (bool, error) {
	input, err := buildCreatePullRequestInput(c)
	if err != nil {
		return false, err
	}

	return s.createChangeset(ctx, c, input)
}

// CreateDraftChangeset creates the given changeset on the code host in draft mode.
func (s GithubSource) CreateDraftChangeset(ctx context.Context, c *Changeset) (bool, error) {
	input, err := buildCreatePullRequestInput(c)
	if err != nil {
		return false, err
	}

	input.Draft = true
	return s.createChangeset(ctx, c, input)
}

func buildCreatePullRequestInput(c *Changeset) (*github.CreatePullRequestInput, error) {
	headRef := gitdomain.AbbreviateRef(c.HeadRef)
	if c.RemoteRepo != c.TargetRepo {
		owner, err := c.RemoteRepo.Metadata.(*github.Repository).Owner()
		if err != nil {
			return nil, err
		}

		headRef = owner + ":" + headRef
	}

	return &github.CreatePullRequestInput{
		RepositoryID: c.TargetRepo.Metadata.(*github.Repository).ID,
		Title:        c.Title,
		Body:         c.Body,
		HeadRefName:  headRef,
		BaseRefName:  gitdomain.AbbreviateRef(c.BaseRef),
	}, nil
}

func (s GithubSource) createChangeset(ctx context.Context, c *Changeset, prInput *github.CreatePullRequestInput) (bool, error) {
	var exists bool
	pr, err := s.client.CreatePullRequest(ctx, prInput)
	if err != nil {
		if err != github.ErrPullRequestAlreadyExists {
			// There is a creation limit (undocumented) in GitHub. When reached, GitHub provides an unclear error
			// message to users. See https://github.com/cli/cli/issues/4801.
			if strings.Contains(err.Error(), "was submitted too quickly") {
				return exists, errors.Wrap(err, "reached GitHub's internal creation limit: see https://docs.sourcegraph.com/admin/config/batch_changes#avoiding-hitting-rate-limits")
			}
			return exists, err
		}
		repo := c.TargetRepo.Metadata.(*github.Repository)
		owner, name, err := github.SplitRepositoryNameWithOwner(repo.NameWithOwner)
		if err != nil {
			return exists, errors.Wrap(err, "getting repo owner and name")
		}
		pr, err = s.client.GetOpenPullRequestByRefs(ctx, owner, name, c.BaseRef, c.HeadRef)
		if err != nil {
			return exists, errors.Wrap(err, "fetching existing PR")
		}
		exists = true
	}

	if err := c.SetMetadata(pr); err != nil {
		return false, errors.Wrap(err, "setting changeset metadata")
	}

	return exists, nil
}

// CloseChangeset closes the given *Changeset on the code host and updates the
// Metadata column in the *batches.Changeset to the newly closed pull request.
func (s GithubSource) CloseChangeset(ctx context.Context, c *Changeset) error {
	pr, ok := c.Changeset.Metadata.(*github.PullRequest)
	if !ok {
		return errors.New("Changeset is not a GitHub pull request")
	}

	err := s.client.ClosePullRequest(ctx, pr)
	if err != nil {
		return err
	}

	return c.Changeset.SetMetadata(pr)
}

// UndraftChangeset will update the Changeset on the source to be not in draft mode anymore.
func (s GithubSource) UndraftChangeset(ctx context.Context, c *Changeset) error {
	pr, ok := c.Changeset.Metadata.(*github.PullRequest)
	if !ok {
		return errors.New("Changeset is not a GitHub pull request")
	}

	err := s.client.MarkPullRequestReadyForReview(ctx, pr)
	if err != nil {
		return err
	}

	return c.Changeset.SetMetadata(pr)
}

// LoadChangeset loads the latest state of the given Changeset from the codehost.
func (s GithubSource) LoadChangeset(ctx context.Context, cs *Changeset) error {
	repo := cs.TargetRepo.Metadata.(*github.Repository)
	number, err := strconv.ParseInt(cs.ExternalID, 10, 64)
	if err != nil {
		return errors.Wrap(err, "parsing changeset external id")
	}

	pr := &github.PullRequest{
		RepoWithOwner: repo.NameWithOwner,
		Number:        number,
	}

	if err := s.client.LoadPullRequest(ctx, pr); err != nil {
		if github.IsNotFound(err) {
			return ChangesetNotFoundError{Changeset: cs}
		}
		return err
	}

	if err := cs.SetMetadata(pr); err != nil {
		return errors.Wrap(err, "setting changeset metadata")
	}

	return nil
}

// UpdateChangeset updates the given *Changeset in the code host.
func (s GithubSource) UpdateChangeset(ctx context.Context, c *Changeset) error {
	pr, ok := c.Changeset.Metadata.(*github.PullRequest)
	if !ok {
		return errors.New("Changeset is not a GitHub pull request")
	}

	updated, err := s.client.UpdatePullRequest(ctx, &github.UpdatePullRequestInput{
		PullRequestID: pr.ID,
		Title:         c.Title,
		Body:          c.Body,
		BaseRefName:   gitdomain.AbbreviateRef(c.BaseRef),
	})

	if err != nil {
		return err
	}

	return c.Changeset.SetMetadata(updated)
}

// ReopenChangeset reopens the given *Changeset on the code host.
func (s GithubSource) ReopenChangeset(ctx context.Context, c *Changeset) error {
	pr, ok := c.Changeset.Metadata.(*github.PullRequest)
	if !ok {
		return errors.New("Changeset is not a GitHub pull request")
	}

	err := s.client.ReopenPullRequest(ctx, pr)
	if err != nil {
		return err
	}

	return c.Changeset.SetMetadata(pr)
}

// CreateComment posts a comment on the Changeset.
func (s GithubSource) CreateComment(ctx context.Context, c *Changeset, text string) error {
	pr, ok := c.Changeset.Metadata.(*github.PullRequest)
	if !ok {
		return errors.New("Changeset is not a GitHub pull request")
	}

	return s.client.CreatePullRequestComment(ctx, pr, text)
}

// MergeChangeset merges a Changeset on the code host, if in a mergeable state.
// If squash is true, a squash-then-merge merge will be performed.
func (s GithubSource) MergeChangeset(ctx context.Context, c *Changeset, squash bool) error {
	pr, ok := c.Changeset.Metadata.(*github.PullRequest)
	if !ok {
		return errors.New("Changeset is not a GitHub pull request")
	}

	if err := s.client.MergePullRequest(ctx, pr, squash); err != nil {
		if github.IsNotMergeable(err) {
			return ChangesetNotMergeableError{ErrorMsg: err.Error()}
		}
		return err
	}

	return c.Changeset.SetMetadata(pr)
}

func (GithubSource) IsPushResponseArchived(s string) bool {
	return strings.Contains(s, "This repository was archived so it is read-only.")
}

func (s GithubSource) GetFork(ctx context.Context, targetRepo *types.Repo, namespace, n *string) (*types.Repo, error) {
	return getGitHubForkInternal(ctx, targetRepo, s.client, namespace, n)
}

type githubClientFork interface {
	Fork(context.Context, string, string, *string, string) (*github.Repository, error)
	GetRepo(context.Context, string, string) (*github.Repository, error)
}

func getGitHubForkInternal(ctx context.Context, targetRepo *types.Repo, client githubClientFork, namespace, n *string) (*types.Repo, error) {
	if namespace != nil && n != nil {
		// Even though we can technically use a single call to `client.Fork` to get or
		// create the fork, it only succeeds if the fork belongs in the currently
		// authenticated user's namespace or if the fork belongs to an organization
		// namespace. So in case the PAT we're using has changed since the last time we
		// tried to get a fork for this repo and it was previously created under a
		// different user's namespace, we'll first separately check if the fork exists.
		if fork, err := client.GetRepo(ctx, *namespace, *n); err == nil && fork != nil {
			return checkAndCopyGitHubRepo(targetRepo, fork)
		}
	}

	tr := targetRepo.Metadata.(*github.Repository)

	targetNamespace, targetName, err := github.SplitRepositoryNameWithOwner(tr.NameWithOwner)
	if err != nil {
		return nil, errors.New("getting target repo namespace")
	}

	var name string
	if n != nil {
		name = *n
	} else {
		name = DefaultForkName(targetNamespace, targetName)
	}

	// `client.Fork` automatically uses the currently authenticated user's namespace if
	// none is provided.
	fork, err := client.Fork(ctx, targetNamespace, targetName, namespace, name)
	if err != nil {
		return nil, errors.Wrap(err, "fetching fork or forking repository")
	}

	return checkAndCopyGitHubRepo(targetRepo, fork)
}

func checkAndCopyGitHubRepo(targetRepo *types.Repo, fork *github.Repository) (*types.Repo, error) {
	tr := targetRepo.Metadata.(*github.Repository)

	if !fork.IsFork {
		return nil, errors.New("repo is not a fork")
	}

	// Now we make a copy of targetRepo, but with its sources and metadata updated to
	// point to the fork
	forkRepo, err := CopyRepoAsFork(targetRepo, fork, tr.NameWithOwner, fork.NameWithOwner)
	if err != nil {
		return nil, errors.Wrap(err, "updating target repo sources and metadata")
	}

	return forkRepo, nil
}
