package repos

import (
	"context"
	"fmt"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"

	multierror "github.com/hashicorp/go-multierror"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/conf/reposource"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// A GitLabSource yields repositories from a single GitLab connection configured
// in Sourcegraph via the external services configuration.
type GitLabSource struct {
	svc     *ExternalService
	config  *schema.GitLabConnection
	exclude map[string]bool
	baseURL *url.URL // URL with path /api/v4 (no trailing slash)
	client  *gitlab.Client
}

// NewGitLabSource returns a new GitLabSource from the given external service.
func NewGitLabSource(svc *ExternalService, cf *httpcli.Factory) (*GitLabSource, error) {
	var c schema.GitLabConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, fmt.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newGitLabSource(svc, &c, cf)
}

func newGitLabSource(svc *ExternalService, c *schema.GitLabConnection, cf *httpcli.Factory) (*GitLabSource, error) {
	baseURL, err := url.Parse(c.Url)
	if err != nil {
		return nil, err
	}
	baseURL = NormalizeBaseURL(baseURL)

	if cf == nil {
		cf = NewHTTPClientFactory()
	}

	var opts []httpcli.Opt
	if c.Certificate != "" {
		pool, err := newCertPool(c.Certificate)
		if err != nil {
			return nil, err
		}
		opts = append(opts, httpcli.NewCertPoolOpt(pool))
	}

	cli, err := cf.Doer(opts...)
	if err != nil {
		return nil, err
	}

	exclude := make(map[string]bool, len(c.Exclude))
	for _, r := range c.Exclude {
		if r.Name != "" {
			exclude[r.Name] = true
		}

		if r.Id != 0 {
			exclude[strconv.Itoa(r.Id)] = true
		}
	}

	return &GitLabSource{
		svc:     svc,
		config:  c,
		exclude: exclude,
		baseURL: baseURL,
		client:  gitlab.NewClientProvider(baseURL, cli).GetPATClient(c.Token, ""),
	}, nil
}

// ListRepos returns all GitLab repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s GitLabSource) ListRepos(ctx context.Context) (repos []*Repo, err error) {
	projs, err := s.listAllProjects(ctx)
	for _, proj := range projs {
		repos = append(repos, s.makeRepo(proj))
	}
	return repos, err
}

// ExternalServices returns a singleton slice containing the external service.
func (s GitLabSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func (s GitLabSource) makeRepo(proj *gitlab.Project) *Repo {
	urn := s.svc.URN()
	return &Repo{
		Name: string(reposource.GitLabRepoName(
			s.config.RepositoryPathPattern,
			s.baseURL.Hostname(),
			proj.PathWithNamespace,
		)),
		URI: string(reposource.GitLabRepoName(
			"",
			s.baseURL.Hostname(),
			proj.PathWithNamespace,
		)),
		ExternalRepo: *gitlab.ExternalRepoSpec(proj, *s.baseURL),
		Description:  proj.Description,
		Fork:         proj.ForkedFromProject != nil,
		Enabled:      true,
		Archived:     proj.Archived,
		Sources: map[string]*SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: s.authenticatedRemoteURL(proj),
			},
		},
		Metadata: proj,
	}
}

// authenticatedRemoteURL returns the GitLab projects's Git remote URL with the configured GitLab personal access
// token inserted in the URL userinfo, for repositories needing authentication.
func (s *GitLabSource) authenticatedRemoteURL(proj *gitlab.Project) string {
	if s.config.GitURLType == "ssh" {
		return proj.SSHURLToRepo // SSH authentication must be provided out-of-band
	}
	if s.config.Token == "" || !proj.RequiresAuthentication() {
		return proj.HTTPURLToRepo
	}
	u, err := url.Parse(proj.HTTPURLToRepo)
	if err != nil {
		log15.Warn("Error adding authentication to GitLab repository Git remote URL.", "url", proj.HTTPURLToRepo, "error", err)
		return proj.HTTPURLToRepo
	}
	// Any username works; "git" is not special.
	u.User = url.UserPassword("git", s.config.Token)
	return u.String()
}

func (s *GitLabSource) excludes(p *gitlab.Project) bool {
	return s.exclude[p.PathWithNamespace] || s.exclude[strconv.Itoa(p.ID)]
}

func (s *GitLabSource) listAllProjects(ctx context.Context) ([]*gitlab.Project, error) {
	type batch struct {
		projs []*gitlab.Project
		err   error
	}

	ch := make(chan batch)

	var wg sync.WaitGroup

	projch := make(chan *schema.GitLabProject)
	for i := 0; i < 5; i++ { // 5 concurrent requests
		wg.Add(1)
		go func() {
			defer wg.Done()
			for p := range projch {
				proj, err := s.client.GetProject(ctx, gitlab.GetProjectOp{
					ID:                p.Id,
					PathWithNamespace: p.Name,
					CommonOp:          gitlab.CommonOp{NoCache: true},
				})

				if err != nil {
					// TODO(tsenart): When implementing dry-run, reconsider alternatives to return
					// 404 errors on external service config validation.
					if gitlab.IsNotFound(err) {
						log15.Warn("skipping missing gitlab.projects entry:", "name", p.Name, "id", p.Id, "err", err)
						continue
					}
					ch <- batch{err: errors.Wrapf(err, "gitlab.projects: id: %d, name: %q", p.Id, p.Name)}
				} else {
					ch <- batch{projs: []*gitlab.Project{proj}}
				}

				time.Sleep(s.client.RateLimit.RecommendedWaitForBackgroundOp(1))
			}
		}()
	}

	wg.Add(1)
	go func() {
		defer wg.Done()
		defer close(projch)
		for _, p := range s.config.Projects {
			select {
			case projch <- p:
			case <-ctx.Done():
				return
			}
		}
	}()

	for _, projectQuery := range s.config.ProjectQuery {
		if projectQuery == "none" {
			continue
		}

		const perPage = 100
		wg.Add(1)
		go func(projectQuery string) {
			defer wg.Done()

			url, err := projectQueryToURL(projectQuery, perPage) // first page URL
			if err != nil {
				ch <- batch{err: errors.Wrapf(err, "invalid GitLab projectQuery=%q", projectQuery)}
				return
			}

			for {
				if err := ctx.Err(); err != nil {
					ch <- batch{err: err}
					return
				}
				projects, nextPageURL, err := s.client.ListProjects(ctx, url)
				if err != nil {
					ch <- batch{err: errors.Wrapf(err, "error listing GitLab projects: url=%q", url)}
					return
				}
				ch <- batch{projs: projects}
				if nextPageURL == nil {
					return
				}
				url = *nextPageURL

				// 0-duration sleep unless nearing rate limit exhaustion
				time.Sleep(s.client.RateLimit.RecommendedWaitForBackgroundOp(1))
			}
		}(projectQuery)
	}

	go func() {
		wg.Wait()
		close(ch)
	}()

	seen := make(map[int]bool)
	errs := new(multierror.Error)
	var projects []*gitlab.Project

	for b := range ch {
		if b.err != nil {
			errs = multierror.Append(errs, b.err)
			continue
		}

		for _, proj := range b.projs {
			if !seen[proj.ID] && !s.excludes(proj) {
				projects = append(projects, proj)
				seen[proj.ID] = true
			}
		}
	}

	return projects, errs.ErrorOrNil()
}

var schemeOrHostNotEmptyErr = errors.New("scheme and host should be empty")

func projectQueryToURL(projectQuery string, perPage int) (string, error) {
	// If all we have is the URL query, prepend "projects"
	if strings.HasPrefix(projectQuery, "?") {
		projectQuery = "projects" + projectQuery
	} else if projectQuery == "" {
		projectQuery = "projects"
	}

	u, err := url.Parse(projectQuery)
	if err != nil {
		return "", err
	}
	if u.Scheme != "" || u.Host != "" {
		return "", schemeOrHostNotEmptyErr
	}
	normalizeQuery(u, perPage)

	return u.String(), nil
}

func normalizeQuery(u *url.URL, perPage int) {
	q := u.Query()
	if q.Get("order_by") == "" && q.Get("sort") == "" {
		// Apply default ordering to get the likely more relevant projects first.
		q.Set("order_by", "last_activity_at")
	}
	q.Set("per_page", strconv.Itoa(perPage))
	u.RawQuery = q.Encode()
}
