package repos

import (
	"context"
	"fmt"
	"net/url"
	"strconv"
	"strings"
	"sync"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

// A BitbucketServerSource yields repositories from a single BitbucketServer connection configured
// in Sourcegraph via the external services configuration.
type BitbucketServerSource struct {
	svc     *types.ExternalService
	config  *schema.BitbucketServerConnection
	exclude excludeFunc
	client  *bitbucketserver.Client
}

var _ Source = &BitbucketServerSource{}
var _ UserSource = &BitbucketServerSource{}

// NewBitbucketServerSource returns a new BitbucketServerSource from the given external service.
// rl is optional
func NewBitbucketServerSource(svc *types.ExternalService, cf *httpcli.Factory) (*BitbucketServerSource, error) {
	var c schema.BitbucketServerConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, fmt.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newBitbucketServerSource(svc, &c, cf)
}

func newBitbucketServerSource(svc *types.ExternalService, c *schema.BitbucketServerConnection, cf *httpcli.Factory) (*BitbucketServerSource, error) {
	if cf == nil {
		cf = httpcli.NewExternalHTTPClientFactory()
	}

	var opts []httpcli.Opt
	if c.Certificate != "" {
		opts = append(opts, httpcli.NewCertPoolOpt(c.Certificate))
	}

	cli, err := cf.Doer(opts...)
	if err != nil {
		return nil, err
	}

	var eb excludeBuilder
	for _, r := range c.Exclude {
		eb.Exact(r.Name)
		eb.Exact(strconv.Itoa(r.Id))
		eb.Pattern(r.Pattern)
	}
	exclude, err := eb.Build()
	if err != nil {
		return nil, err
	}

	client, err := bitbucketserver.NewClient(c, cli)
	if err != nil {
		return nil, err
	}

	return &BitbucketServerSource{
		svc:     svc,
		config:  c,
		exclude: exclude,
		client:  client,
	}, nil
}

// ListRepos returns all BitbucketServer repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s BitbucketServerSource) ListRepos(ctx context.Context, results chan SourceResult) {
	s.listAllRepos(ctx, results)
}

func (s BitbucketServerSource) WithAuthenticator(a auth.Authenticator) (Source, error) {
	switch a.(type) {
	case *auth.OAuthBearerToken,
		*auth.OAuthBearerTokenWithSSH,
		*auth.BasicAuth,
		*auth.BasicAuthWithSSH,
		*bitbucketserver.SudoableOAuthClient:
		break

	default:
		return nil, newUnsupportedAuthenticatorError("BitbucketServerSource", a)
	}

	sc := s
	sc.client = sc.client.WithAuthenticator(a)

	return &sc, nil
}

// ExternalServices returns a singleton slice containing the external service.
func (s BitbucketServerSource) ExternalServices() types.ExternalServices {
	return types.ExternalServices{s.svc}
}

func (s BitbucketServerSource) makeRepo(repo *bitbucketserver.Repo, isArchived bool) *types.Repo {
	host, err := url.Parse(s.config.Url)
	if err != nil {
		// This should never happen
		panic(errors.Errorf("malformed bitbucket config, invalid URL: %q, error: %s", s.config.Url, err))
	}
	host = extsvc.NormalizeBaseURL(host)

	// Name
	project := "UNKNOWN"
	if repo.Project != nil {
		project = repo.Project.Key
	}

	// Clone URL
	var cloneURL string
	for _, l := range repo.Links.Clone {
		if l.Name == "ssh" && s.config.GitURLType == "ssh" {
			cloneURL = l.Href
			break
		}
		if l.Name == "http" {
			cloneURL = setUserinfoBestEffort(l.Href, s.config.Username, "")
			// No break, so that we fallback to http in case of ssh missing
			// with GitURLType == "ssh"
		}
	}

	urn := s.svc.URN()

	return &types.Repo{
		Name: reposource.BitbucketServerRepoName(
			s.config.RepositoryPathPattern,
			host.Hostname(),
			project,
			repo.Slug,
		),
		URI: string(reposource.BitbucketServerRepoName(
			"",
			host.Hostname(),
			project,
			repo.Slug,
		)),
		ExternalRepo: api.ExternalRepoSpec{
			ID:          strconv.Itoa(repo.ID),
			ServiceType: extsvc.TypeBitbucketServer,
			ServiceID:   host.String(),
		},
		Description: repo.Name,
		Fork:        repo.Origin != nil,
		Archived:    isArchived,
		Private:     !repo.Public,
		Sources: map[string]*types.SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: cloneURL,
			},
		},
		Metadata: repo,
	}
}

func (s *BitbucketServerSource) excludes(r *bitbucketserver.Repo) bool {
	name := r.Slug
	if r.Project != nil {
		name = r.Project.Key + "/" + name
	}
	if r.State != "AVAILABLE" ||
		s.exclude(name) ||
		s.exclude(strconv.Itoa(r.ID)) ||
		(s.config.ExcludePersonalRepositories && r.IsPersonalRepository()) {
		return true
	}

	return false
}

func (s *BitbucketServerSource) listAllRepos(ctx context.Context, results chan SourceResult) {
	// "archived" label is a convention used at some customers for indicating
	// a repository is archived (like github's archived state). This is not
	// returned in the normal repository listing endpoints, so we need to
	// fetch it separately.
	archived, err := s.listAllLabeledRepos(ctx, "archived")
	if err != nil {
		results <- SourceResult{Source: s, Err: errors.Wrap(err, "failed to list repos with archived label")}
		return
	}

	type batch struct {
		repos []*bitbucketserver.Repo
		err   error
	}

	ch := make(chan batch)

	var wg sync.WaitGroup

	wg.Add(1)
	go func() {
		defer wg.Done()

		// Admins normally add to end of lists, so end of list most likely has
		// new repos => stream them first.
		for i := len(s.config.Repos) - 1; i >= 0; i-- {
			name := s.config.Repos[i]
			ps := strings.SplitN(name, "/", 2)
			if len(ps) != 2 {
				ch <- batch{err: errors.Errorf("bitbucketserver.repos: name=%q", name)}
				continue
			}

			projectKey, repoSlug := ps[0], ps[1]
			repo, err := s.client.Repo(ctx, projectKey, repoSlug)
			if err != nil {
				// TODO(tsenart): When implementing dry-run, reconsider alternatives to return
				// 404 errors on external service config validation.
				if bitbucketserver.IsNotFound(err) {
					log15.Warn("skipping missing bitbucketserver.repos entry:", "name", name, "err", err)
					continue
				}
				ch <- batch{err: errors.Wrapf(err, "bitbucketserver.repos: name: %q", name)}
			} else {
				ch <- batch{repos: []*bitbucketserver.Repo{repo}}
			}
		}
	}()

	for _, q := range s.config.RepositoryQuery {
		switch q {
		case "none":
			continue
		case "all":
			q = "" // No filters.
		}

		wg.Add(1)
		go func(q string) {
			defer wg.Done()

			next := &bitbucketserver.PageToken{Limit: 1000}
			for next.HasMore() {
				repos, page, err := s.client.Repos(ctx, next, q)
				if err != nil {
					ch <- batch{err: errors.Wrapf(err, "bitbucketserver.repositoryQuery: query=%q, page=%+v", q, next)}
					break
				}

				ch <- batch{repos: repos}
				next = page
			}
		}(q)
	}

	go func() {
		wg.Wait()
		close(ch)
	}()

	seen := make(map[int]bool)
	for r := range ch {
		if r.err != nil {
			results <- SourceResult{Source: s, Err: r.err}
			continue
		}

		for _, repo := range r.repos {
			if !seen[repo.ID] && !s.excludes(repo) {
				_, isArchived := archived[repo.ID]
				results <- SourceResult{Source: s, Repo: s.makeRepo(repo, isArchived)}
				seen[repo.ID] = true
			}
		}
	}
}

func (s *BitbucketServerSource) listAllLabeledRepos(ctx context.Context, label string) (map[int]struct{}, error) {
	ids := map[int]struct{}{}
	next := &bitbucketserver.PageToken{Limit: 1000}
	for next.HasMore() {
		repos, page, err := s.client.LabeledRepos(ctx, next, label)
		if err != nil {
			// If the instance doesn't have the label then no repos are
			// labeled. Older versions of bitbucket do not support labels, so
			// they too have no labelled repos.
			if bitbucketserver.IsNoSuchLabel(err) || bitbucketserver.IsNotFound(err) {
				// treat as empty
				return ids, nil
			}
			return nil, err
		}

		for _, r := range repos {
			ids[r.ID] = struct{}{}
		}

		next = page
	}
	return ids, nil
}

// AuthenticatedUsername uses the underlying bitbucketserver.Client to get the
// username belonging to the credentials associated with the
// BitbucketServerSource.
func (s *BitbucketServerSource) AuthenticatedUsername(ctx context.Context) (string, error) {
	return s.client.AuthenticatedUsername(ctx)
}

func (s *BitbucketServerSource) ValidateAuthenticator(ctx context.Context) error {
	_, err := s.client.AuthenticatedUsername(ctx)
	return err
}
