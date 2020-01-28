package repos

import (
	"context"
	"fmt"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"sync"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// A BitbucketServerSource yields repositories from a single BitbucketServer connection configured
// in Sourcegraph via the external services configuration.
type BitbucketServerSource struct {
	svc             *ExternalService
	config          *schema.BitbucketServerConnection
	exclude         map[string]bool
	excludePatterns []*regexp.Regexp
	client          *bitbucketserver.Client
}

// NewBitbucketServerSource returns a new BitbucketServerSource from the given external service.
func NewBitbucketServerSource(svc *ExternalService, cf *httpcli.Factory) (*BitbucketServerSource, error) {
	var c schema.BitbucketServerConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, fmt.Errorf("external service id=%d config error: %s", svc.ID, err)
	}
	return newBitbucketServerSource(svc, &c, cf)
}

func newBitbucketServerSource(svc *ExternalService, c *schema.BitbucketServerConnection, cf *httpcli.Factory) (*BitbucketServerSource, error) {
	baseURL, err := url.Parse(c.Url)
	if err != nil {
		return nil, err
	}
	baseURL = extsvc.NormalizeBaseURL(baseURL)

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

	exclude := make(map[string]bool, len(c.Exclude))
	var excludePatterns []*regexp.Regexp
	for _, r := range c.Exclude {
		if r.Name != "" {
			exclude[strings.ToLower(r.Name)] = true
		}

		if r.Id != 0 {
			exclude[strconv.Itoa(r.Id)] = true
		}

		if r.Pattern != "" {
			re, err := regexp.Compile(r.Pattern)
			if err != nil {
				return nil, err
			}
			excludePatterns = append(excludePatterns, re)
		}
	}

	client := bitbucketserver.NewClient(baseURL, cli)
	client.Token = c.Token
	client.Username = c.Username
	client.Password = c.Password

	return &BitbucketServerSource{
		svc:             svc,
		config:          c,
		exclude:         exclude,
		excludePatterns: excludePatterns,
		client:          client,
	}, nil
}

// ListRepos returns all BitbucketServer repositories accessible to all connections configured
// in Sourcegraph via the external services configuration.
func (s BitbucketServerSource) ListRepos(ctx context.Context, results chan SourceResult) {
	s.listAllRepos(ctx, results)
}

var _ ChangesetSource = BitbucketServerSource{}

// CreateChangeset creates the given *Changeset in the code host.
func (s BitbucketServerSource) CreateChangeset(ctx context.Context, c *Changeset) (bool, error) {
	var exists bool

	repo := c.Repo.Metadata.(*bitbucketserver.Repo)

	pr := &bitbucketserver.PullRequest{Title: c.Title, Description: c.Body}

	pr.ToRef.Repository.Slug = repo.Slug
	pr.ToRef.Repository.Project.Key = repo.Project.Key
	pr.ToRef.ID = git.EnsureRefPrefix(c.BaseRef)

	pr.FromRef.Repository.Slug = repo.Slug
	pr.FromRef.Repository.Project.Key = repo.Project.Key
	pr.FromRef.ID = git.EnsureRefPrefix(c.HeadRef)

	err := s.client.CreatePullRequest(ctx, pr)
	if err != nil {
		if ae, ok := err.(*bitbucketserver.ErrAlreadyExists); ok && ae != nil {
			if ae.Existing == nil {
				return exists, fmt.Errorf("existing PR is nil")
			}
			log15.Info("Existing PR extracted", "ID", ae.Existing.ID)
			pr = ae.Existing
			exists = true
		} else {
			return exists, err
		}
	}

	c.Changeset.Metadata = pr
	c.Changeset.ExternalID = strconv.FormatInt(int64(pr.ID), 10)
	c.Changeset.ExternalServiceType = bitbucketserver.ServiceType

	return exists, nil
}

// CloseChangeset closes the given *Changeset on the code host and updates the
// Metadata column in the *a8n.Changeset to the newly closed pull request.
func (s BitbucketServerSource) CloseChangeset(ctx context.Context, c *Changeset) error {
	pr, ok := c.Changeset.Metadata.(*bitbucketserver.PullRequest)
	if !ok {
		return errors.New("Changeset is not a Bitbucket Server pull request")
	}

	err := s.client.DeclinePullRequest(ctx, pr)
	if err != nil {
		return err
	}

	c.Changeset.Metadata = pr

	return nil
}

// LoadChangesets loads the latest state of the given Changesets from the codehost.
func (s BitbucketServerSource) LoadChangesets(ctx context.Context, cs ...*Changeset) error {
	var notFound []*Changeset

	for i := range cs {
		repo := cs[i].Repo.Metadata.(*bitbucketserver.Repo)
		number, err := strconv.Atoi(cs[i].ExternalID)
		if err != nil {
			return err
		}

		pr := &bitbucketserver.PullRequest{ID: number}
		pr.ToRef.Repository.Slug = repo.Slug
		pr.ToRef.Repository.Project.Key = repo.Project.Key

		err = s.client.LoadPullRequest(ctx, pr)
		if err != nil {
			if bitbucketserver.IsNotFound(err) {
				notFound = append(notFound, cs[i])
				if cs[i].Changeset.Metadata == nil {
					cs[i].Changeset.Metadata = pr
				}
				continue
			}

			return err
		}

		err = s.client.LoadPullRequestActivities(ctx, pr)
		if err != nil {
			return err
		}

		cs[i].Changeset.Metadata = pr
	}

	if len(notFound) > 0 {
		return ChangesetsNotFoundError{Changesets: notFound}
	}

	return nil
}

func (s BitbucketServerSource) UpdateChangeset(ctx context.Context, c *Changeset) error {
	pr, ok := c.Changeset.Metadata.(*bitbucketserver.PullRequest)
	if !ok {
		return errors.New("Changeset is not a Bitbucket Server pull request")
	}

	update := &bitbucketserver.UpdatePullRequestInput{
		PullRequestID: strconv.Itoa(pr.ID),
		Title:         c.Title,
		Description:   c.Body,
		Version:       pr.Version,
	}
	update.ToRef.ID = c.BaseRef
	update.ToRef.Repository.Slug = pr.ToRef.Repository.Slug
	update.ToRef.Repository.Project.Key = pr.ToRef.Repository.Project.Key

	updated, err := s.client.UpdatePullRequest(ctx, update)
	if err != nil {
		return err
	}

	c.Changeset.Metadata = updated
	return nil
}

// ExternalServices returns a singleton slice containing the external service.
func (s BitbucketServerSource) ExternalServices() ExternalServices {
	return ExternalServices{s.svc}
}

func (s BitbucketServerSource) makeRepo(repo *bitbucketserver.Repo, isArchived bool) *Repo {
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
			var password string
			if s.config.Token != "" {
				password = s.config.Token // prefer personal access token
			} else {
				password = s.config.Password
			}
			cloneURL = setUserinfoBestEffort(l.Href, s.config.Username, password)
			// No break, so that we fallback to http in case of ssh missing
			// with GitURLType == "ssh"
		}
	}

	// Repo Links
	// var links *protocol.RepoLinks
	// for _, l := range repo.Links.Self {
	// 	root := strings.TrimSuffix(l.Href, "/browse")
	// 	links = &protocol.RepoLinks{
	// 		Root:   l.Href,
	// 		Tree:   root + "/browse/{path}?at={rev}",
	// 		Blob:   root + "/browse/{path}?at={rev}",
	// 		Commit: root + "/commits/{commit}",
	// 	}
	// 	break
	// }

	urn := s.svc.URN()

	return &Repo{
		Name: string(reposource.BitbucketServerRepoName(
			s.config.RepositoryPathPattern,
			host.Hostname(),
			project,
			repo.Slug,
		)),
		URI: string(reposource.BitbucketServerRepoName(
			"",
			host.Hostname(),
			project,
			repo.Slug,
		)),
		ExternalRepo: api.ExternalRepoSpec{
			ID:          strconv.Itoa(repo.ID),
			ServiceType: bitbucketserver.ServiceType,
			ServiceID:   host.String(),
		},
		Description: repo.Name,
		Fork:        repo.Origin != nil,
		Archived:    isArchived,
		Sources: map[string]*SourceInfo{
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
		s.exclude[strings.ToLower(name)] ||
		s.exclude[strconv.Itoa(r.ID)] ||
		(s.config.ExcludePersonalRepositories && r.IsPersonalRepository()) {
		return true
	}

	for _, re := range s.excludePatterns {
		if re.MatchString(name) {
			return true
		}
	}
	return false
}

func (s *BitbucketServerSource) listAllRepos(ctx context.Context, results chan SourceResult) {
	// "archived" label is a convention used at some customers for indicating
	// a repository is archived (like github's archived state). This is not
	// returned in the normal repository listing endpoints, so we need to
	// fetch it seperately.
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
			if bitbucketserver.IsNoSuchLabel(err) {
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
