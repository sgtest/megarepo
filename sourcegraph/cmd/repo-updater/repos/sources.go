package repos

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"

	multierror "github.com/hashicorp/go-multierror"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/conf/reposource"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/github"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/gitolite"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
)

// A Sourcer converts the given ExternalServices to Sources
// whose yielded Repos should be synced.
type Sourcer func(...*ExternalService) (Sources, error)

// NewSourcer returns a Sourcer that converts the given ExternalServices
// into Sources that use the provided httpcli.Factory to create the
// http.Clients needed to contact the respective upstream code host APIs.
//
// Deleted external services are ignored.
//
// The provided decorator functions will be applied to each Source.
func NewSourcer(cf httpcli.Factory, decs ...func(Source) Source) Sourcer {
	return func(svcs ...*ExternalService) (Sources, error) {
		srcs := make([]Source, 0, len(svcs))
		errs := new(multierror.Error)

		for _, svc := range svcs {
			if svc.IsDeleted() {
				continue
			}

			src, err := NewSource(svc, cf)
			if err != nil {
				errs = multierror.Append(errs, err)
				continue
			}

			for _, dec := range decs {
				src = dec(src)
			}

			srcs = append(srcs, src)
		}

		if !includesGitHubDotComSource(srcs) {
			// add a GitHub.com source by default, to support navigating to URL
			// paths like /github.com/foo/bar to auto-add that repository. This
			// source returns nothing for ListRepos. However, in the future we
			// intend to use it in repoLookup.
			src, err := NewGithubDotComSource(cf)
			srcs, errs = append(srcs, src), multierror.Append(errs, err)
		}

		return srcs, errs.ErrorOrNil()
	}
}

// NewSource returns a repository yielding Source from the given ExternalService configuration.
func NewSource(svc *ExternalService, cf httpcli.Factory) (Source, error) {
	switch strings.ToLower(svc.Kind) {
	case "github":
		return NewGithubSource(svc, cf)
	case "gitlab":
		return NewGitLabSource(svc, cf)
	case "bitbucketserver":
		return NewBitbucketServerSource(svc, cf)
	case "gitolite":
		return NewGitoliteSource(svc, cf)
	case "other":
		return NewOtherSource(svc)
	default:
		panic(fmt.Sprintf("source not implemented for external service kind %q", svc.Kind))
	}
}

func includesGitHubDotComSource(srcs []Source) bool {
	for _, svc := range Sources(srcs).ExternalServices() {
		if !strings.EqualFold(svc.Kind, "GITHUB") {
			continue
		} else if cfg, err := svc.Configuration(); err != nil {
			continue
		} else if u, err := url.Parse(cfg.(*schema.GitHubConnection).Url); err != nil {
			continue
		} else if strings.HasSuffix(u.Hostname(), "github.com") {
			return true
		}
	}
	return false
}

// sourceTimeout is the default timeout to use on Source.ListRepos
const sourceTimeout = 10 * time.Minute

// A Source yields repositories to be stored and analysed by Sourcegraph.
// Successive calls to its ListRepos method may yield different results.
type Source interface {
	// ListRepos returns all the repos a source yields.
	ListRepos(context.Context) ([]*Repo, error)
	// ExternalServices returns the ExternalServices for the Source.
	ExternalServices() ExternalServices
}

// Sources is a list of Sources that implements the Source interface.
type Sources []Source

// ListRepos lists all the repos of all the sources and returns the
// aggregate result.
func (srcs Sources) ListRepos(ctx context.Context) ([]*Repo, error) {
	if len(srcs) == 0 {
		return nil, nil
	}

	type result struct {
		src   Source
		repos []*Repo
		err   error
	}

	ch := make(chan result, len(srcs))
	for _, src := range srcs {
		go func(src Source) {
			if repos, err := src.ListRepos(ctx); err != nil {
				ch <- result{src: src, err: err}
			} else {
				ch <- result{src: src, repos: repos}
			}
		}(src)
	}

	var repos []*Repo
	errs := new(multierror.Error)

	for i := 0; i < cap(ch); i++ {
		if r := <-ch; r.err != nil {
			errs = multierror.Append(errs, r.err)
		} else {
			repos = append(repos, r.repos...)
		}
	}

	return repos, errs.ErrorOrNil()
}

// ExternalServices returns the ExternalServices from the given Sources.
func (srcs Sources) ExternalServices() ExternalServices {
	es := make(ExternalServices, 0, len(srcs))
	for _, src := range srcs {
		es = append(es, src.ExternalServices()...)
	}
	return es
}

// A GithubSource yields repositories from a single Github connection configured
// in Sourcegraph via the external services configuration.
type GithubSource struct {
	svc  *ExternalService
	conn *githubConnection
}

// NewGithubSource returns a new GithubSource from the given external service.
func NewGithubSource(svc *ExternalService, cf httpcli.Factory) (*GithubSource, error) {
	var c schema.GitHubConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, fmt.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newGithubSource(svc, &c, cf)
}

// NewGithubDotComSource returns a GithubSource for github.com, meant to be added
// to the list of sources in Sourcer when one isn't already configured in order to
// support navigating to URL paths like /github.com/foo/bar to auto-add that repository.
func NewGithubDotComSource(cf httpcli.Factory) (*GithubSource, error) {
	svc := ExternalService{Kind: "GITHUB"}
	return newGithubSource(&svc, &schema.GitHubConnection{
		RepositoryQuery:             []string{"none"}, // don't try to list all repositories during syncs
		Url:                         "https://github.com",
		InitialRepositoryEnablement: true,
	}, cf)
}

func newGithubSource(svc *ExternalService, c *schema.GitHubConnection, cf httpcli.Factory) (*GithubSource, error) {
	conn, err := newGitHubConnection(c, cf)
	if err != nil {
		return nil, err
	}
	return &GithubSource{svc: svc, conn: conn}, nil
}

// ListRepos returns all Github repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s GithubSource) ListRepos(ctx context.Context) (repos []*Repo, err error) {
	rs, err := s.conn.listAllRepositories(ctx)
	for _, r := range rs {
		repos = append(repos, githubRepoToRepo(s.svc, r, s.conn))
	}
	return repos, err
}

// ExternalServices returns a singleton slice containing the external service.
func (s GithubSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func githubRepoToRepo(
	svc *ExternalService,
	ghrepo *github.Repository,
	conn *githubConnection,
) *Repo {
	urn := svc.URN()
	return &Repo{
		Name:         string(githubRepositoryToRepoPath(conn, ghrepo)),
		ExternalRepo: *github.ExternalRepoSpec(ghrepo, *conn.baseURL),
		Description:  ghrepo.Description,
		Fork:         ghrepo.IsFork,
		Enabled:      true,
		Archived:     ghrepo.IsArchived,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: conn.authenticatedRemoteURL(ghrepo),
			},
		},
		Metadata: ghrepo,
	}
}

// A GitLabSource yields repositories from a single GitLab connection configured
// in Sourcegraph via the external services configuration.
type GitLabSource struct {
	svc  *ExternalService
	conn *gitlabConnection
}

// NewGitLabSource returns a new GitLabSource from the given external service.
func NewGitLabSource(svc *ExternalService, cf httpcli.Factory) (*GitLabSource, error) {
	var c schema.GitLabConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, fmt.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newGitLabSource(svc, &c, cf)
}

func newGitLabSource(svc *ExternalService, c *schema.GitLabConnection, cf httpcli.Factory) (*GitLabSource, error) {
	conn, err := newGitLabConnection(c, cf)
	if err != nil {
		return nil, err
	}
	return &GitLabSource{svc: svc, conn: conn}, nil
}

// ListRepos returns all GitLab repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s GitLabSource) ListRepos(ctx context.Context) (repos []*Repo, err error) {
	projs, err := s.conn.listAllProjects(ctx)
	for _, proj := range projs {
		repos = append(repos, gitlabProjectToRepo(s.svc, proj, s.conn))
	}
	return repos, err
}

// ExternalServices returns a singleton slice containing the external service.
func (s GitLabSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func gitlabProjectToRepo(
	svc *ExternalService,
	proj *gitlab.Project,
	conn *gitlabConnection,
) *Repo {
	urn := svc.URN()
	return &Repo{
		Name:         string(gitlabProjectToRepoPath(conn, proj)),
		ExternalRepo: *gitlab.ExternalRepoSpec(proj, *conn.baseURL),
		Description:  proj.Description,
		Fork:         proj.ForkedFromProject != nil,
		Enabled:      true,
		Archived:     proj.Archived,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: conn.authenticatedRemoteURL(proj),
			},
		},
		Metadata: proj,
	}
}

// A BitbucketServerSource yields repositories from a single BitbucketServer connection configured
// in Sourcegraph via the external services configuration.
type BitbucketServerSource struct {
	svc  *ExternalService
	conn *bitbucketServerConnection
}

// NewBitbucketServerSource returns a new BitbucketServerSource from the given external service.
func NewBitbucketServerSource(svc *ExternalService, cf httpcli.Factory) (*BitbucketServerSource, error) {
	var c schema.BitbucketServerConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, fmt.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newBitbucketServerSource(svc, &c, cf)
}

func newBitbucketServerSource(svc *ExternalService, c *schema.BitbucketServerConnection, cf httpcli.Factory) (*BitbucketServerSource, error) {
	conn, err := newBitbucketServerConnection(c, cf)
	if err != nil {
		return nil, err
	}
	return &BitbucketServerSource{svc: svc, conn: conn}, nil
}

// ListRepos returns all BitbucketServer repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s BitbucketServerSource) ListRepos(ctx context.Context) (repos []*Repo, err error) {
	rs, err := s.conn.listAllRepos(ctx)
	for _, r := range rs {
		repos = append(repos, bitbucketserverRepoToRepo(s.svc, r, s.conn))
	}
	return repos, err
}

// ExternalServices returns a singleton slice containing the external service.
func (s BitbucketServerSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func bitbucketserverRepoToRepo(
	svc *ExternalService,
	repo *bitbucketserver.Repo,
	conn *bitbucketServerConnection,
) *Repo {
	info := bitbucketServerRepoInfo(conn.config, repo)
	urn := svc.URN()
	return &Repo{
		Name:         string(info.Name),
		ExternalRepo: *info.ExternalRepo,
		Description:  info.Description,
		Fork:         info.Fork,
		Enabled:      true,
		Archived:     info.Archived,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: info.VCS.URL,
			},
		},
		Metadata: repo,
	}
}

// A GitoliteSource yields repositories from a single Gitolite connection configured
// in Sourcegraph via the external services configuration.
type GitoliteSource struct {
	svc  *ExternalService
	conn *schema.GitoliteConnection
	// We ask gitserver to talk to gitolite because it holds the ssh keys
	// required for authentication.
	cli       *gitserver.Client
	blacklist *regexp.Regexp
}

// NewGitoliteSource returns a new GitoliteSource from the given external service.
func NewGitoliteSource(svc *ExternalService, cf httpcli.Factory) (*GitoliteSource, error) {
	var c schema.GitoliteConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, errors.Wrapf(err, "external service id=%d config error", svc.ID)
	}

	hc, err := cf.NewClient(func(c *http.Client) error {
		if tr, ok := c.Transport.(*http.Transport); ok {
			tr.MaxIdleConnsPerHost = 500
		}
		return nil
	})

	if err != nil {
		return nil, err
	}

	var blacklist *regexp.Regexp
	if c.Blacklist != "" {
		if blacklist, err = regexp.Compile(c.Blacklist); err != nil {
			return nil, err
		}
	}

	return &GitoliteSource{
		svc:       svc,
		conn:      &c,
		cli:       gitserver.NewClient(hc),
		blacklist: blacklist,
	}, nil
}

// ListRepos returns all Gitolite repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s *GitoliteSource) ListRepos(ctx context.Context) ([]*Repo, error) {
	all, err := s.cli.ListGitolite(ctx, s.conn.Host)
	if err != nil {
		return nil, err
	}

	repos := make([]*Repo, 0, len(all))
	for _, r := range all {
		if repo := gitoliteRepoToRepo(s.svc, r, s.conn); !s.exclude(repo) {
			repos = append(repos, repo)
		}
	}

	return repos, nil
}

// ExternalServices returns a singleton slice containing the external service.
func (s GitoliteSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func (s GitoliteSource) exclude(r *Repo) bool {
	return strings.ContainsAny(r.Name, "\\^$|()[]*?{},") ||
		(s.blacklist != nil && s.blacklist.MatchString(r.Name))
}

func gitoliteRepoToRepo(
	svc *ExternalService,
	repo *gitolite.Repo,
	conn *schema.GitoliteConnection,
) *Repo {
	urn := svc.URN()
	return &Repo{
		Name:         string(reposource.GitoliteRepoName(conn.Prefix, repo.Name)),
		ExternalRepo: *gitolite.ExternalRepoSpec(repo, gitolite.ServiceID(conn.Host)),
		Enabled:      true,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: repo.URL,
			},
		},
		Metadata: repo,
	}
}

// A OtherSource yields repositories from a single Other connection configured
// in Sourcegraph via the external services configuration.
type OtherSource struct {
	svc  *ExternalService
	conn *schema.OtherExternalServiceConnection
}

// NewOtherSource returns a new OtherSource from the given external service.
func NewOtherSource(svc *ExternalService) (*OtherSource, error) {
	var c schema.OtherExternalServiceConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, errors.Wrapf(err, "external service id=%d config error", svc.ID)
	}
	return &OtherSource{svc: svc, conn: &c}, nil
}

// ListRepos returns all Other repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s OtherSource) ListRepos(ctx context.Context) ([]*Repo, error) {
	urls, err := s.cloneURLs()
	if err != nil {
		return nil, err
	}

	urn := s.svc.URN()
	repos := make([]*Repo, 0, len(urls))
	for _, u := range urls {
		repos = append(repos, otherRepoFromCloneURL(urn, u))
	}

	return repos, nil
}

// ExternalServices returns a singleton slice containing the external service.
func (s OtherSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func (s OtherSource) cloneURLs() ([]*url.URL, error) {
	if len(s.conn.Repos) == 0 {
		return nil, nil
	}

	var base *url.URL
	if s.conn.Url != "" {
		var err error
		if base, err = url.Parse(s.conn.Url); err != nil {
			return nil, err
		}
	}

	cloneURLs := make([]*url.URL, 0, len(s.conn.Repos))
	for _, repo := range s.conn.Repos {
		cloneURL, err := otherRepoCloneURL(base, repo)
		if err != nil {
			return nil, err
		}
		cloneURLs = append(cloneURLs, cloneURL)
	}

	return cloneURLs, nil
}

func otherRepoCloneURL(base *url.URL, repo string) (*url.URL, error) {
	if base == nil {
		return url.Parse(repo)
	}
	return base.Parse(repo)
}

var otherRepoNameReplacer = strings.NewReplacer(":", "-", "@", "-", "//", "")

func otherRepoName(cloneURL *url.URL) string {
	u := *cloneURL
	u.User = nil
	u.Scheme = ""
	u.RawQuery = ""
	u.Fragment = ""
	return otherRepoNameReplacer.Replace(u.String())
}

func otherRepoFromCloneURL(urn string, u *url.URL) *Repo {
	repoURL := u.String()
	repoName := otherRepoName(u)
	u.Path, u.RawQuery = "", ""
	serviceID := u.String()

	return &Repo{
		Name: repoName,
		ExternalRepo: api.ExternalRepoSpec{
			ID:          string(repoName),
			ServiceType: "other",
			ServiceID:   serviceID,
		},
		Enabled: true,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: repoURL,
			},
		},
	}
}
