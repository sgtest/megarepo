package repos

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/url"
	"os"
	"reflect"
	"strconv"
	"strings"
	"time"

	multierror "github.com/hashicorp/go-multierror"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/atomicvalue"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/conf/reposource"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/github"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

var githubConnections = func() *atomicvalue.Value {
	c := atomicvalue.New()
	c.Set(func() interface{} {
		return []*githubConnection{}
	})
	return c
}()

// SyncGitHubConnections periodically syncs connections from
// the Frontend API.
func SyncGitHubConnections(ctx context.Context) {
	t := time.NewTicker(configWatchInterval)
	var lastGitHubConf []*schema.GitHubConnection
	for range t.C {
		githubConf, err := conf.GitHubConfigs(ctx)
		if err != nil {
			log15.Error("unable to fetch GitHub configs", "err", err)
			continue
		}

		var hasGitHubDotComConnection bool
		for _, c := range githubConf {
			u, _ := url.Parse(c.Url)
			if u != nil && (u.Hostname() == "github.com" || u.Hostname() == "www.github.com" || u.Hostname() == "api.github.com") {
				hasGitHubDotComConnection = true
				break
			}
		}
		if !hasGitHubDotComConnection {
			// Add a GitHub.com entry by default, to support navigating to URL paths like
			// /github.com/foo/bar to auto-add that repository.
			githubConf = append(githubConf, &schema.GitHubConnection{
				RepositoryQuery:             []string{"none"}, // don't try to list all repositories during syncs
				Url:                         "https://github.com",
				InitialRepositoryEnablement: true,
			})
		}

		if reflect.DeepEqual(githubConf, lastGitHubConf) {
			continue
		}
		lastGitHubConf = githubConf

		var conns []*githubConnection
		for _, c := range githubConf {
			conn, err := newGitHubConnection(c, nil)
			if err != nil {
				log15.Error("Error processing configured GitHub connection. Skipping it.", "url", c.Url, "error", err)
				continue
			}
			conns = append(conns, conn)
		}

		githubConnections.Set(func() interface{} {
			return conns
		})

		gitHubRepositorySyncWorker.restart()
	}
}

// getGitHubConnection returns the GitHub connection (config + API client) that is responsible for
// the repository specified by the args.
func getGitHubConnection(args protocol.RepoLookupArgs) (*githubConnection, error) {
	githubConnections := githubConnections.Get().([]*githubConnection)
	if args.ExternalRepo != nil && args.ExternalRepo.ServiceType == github.ServiceType {
		// Look up by external repository spec.
		skippedBecauseNoAuth := false
		for _, conn := range githubConnections {
			if args.ExternalRepo.ServiceID == conn.baseURL.String() {
				if canUseGraphQLAPI := conn.config.Token != ""; !canUseGraphQLAPI { // GraphQL API requires authentication
					skippedBecauseNoAuth = true
					continue
				}
				return conn, nil
			}
		}

		if !skippedBecauseNoAuth {
			return nil, errors.Wrap(github.ErrNotFound, fmt.Sprintf("no configured GitHub connection with URL: %q", args.ExternalRepo.ServiceID))
		}
	}

	if args.Repo != "" {
		// Look up by repository name.
		repo := strings.ToLower(string(args.Repo))
		for _, conn := range githubConnections {
			if strings.HasPrefix(repo, conn.originalHostname+"/") {
				return conn, nil
			}
		}
	}

	return nil, nil
}

// GetGitHubRepositoryMock is set by tests that need to mock GetGitHubRepository.
var GetGitHubRepositoryMock func(args protocol.RepoLookupArgs) (repo *protocol.RepoInfo, authoritative bool, err error)

var (
	bypassGitHubAPI, _       = strconv.ParseBool(os.Getenv("BYPASS_GITHUB_API"))
	minGitHubAPIRateLimit, _ = strconv.Atoi(os.Getenv("GITHUB_API_MIN_RATE_LIMIT"))

	// ErrGitHubAPITemporarilyUnavailable is returned by GetGitHubRepository when the GitHub API is
	// unavailable.
	ErrGitHubAPITemporarilyUnavailable = errors.New("the GitHub API is temporarily unavailable")
)

func init() {
	if v, _ := strconv.ParseBool(os.Getenv("OFFLINE")); v {
		bypassGitHubAPI = true
	}
}

// GetGitHubRepository queries a configured GitHub connection endpoint for information about the
// specified repository.
//
// If args.Repo refers to a repository that is not known to be on a configured GitHub connection's
// host, it returns authoritative == false.
func GetGitHubRepository(ctx context.Context, args protocol.RepoLookupArgs) (repo *protocol.RepoInfo, authoritative bool, err error) {
	if GetGitHubRepositoryMock != nil {
		return GetGitHubRepositoryMock(args)
	}

	conn, err := getGitHubConnection(args)
	if err != nil {
		return nil, true, err // refers to a GitHub repo but the host is not configured
	}
	if conn == nil {
		return nil, false, nil // refers to a non-GitHub repo
	}

	// Support bypassing GitHub API, for rate limit evasion.
	var bypassReason string
	bypass := bypassGitHubAPI
	if bypass {
		bypassReason = "manual bypass env var BYPASS_GITHUB_API=1 is set"
	}
	if !bypass && minGitHubAPIRateLimit > 0 {
		remaining, reset, _, known := conn.client.RateLimit.Get()
		// If we're below the min rate limit, bypass the GitHub API. But if the rate limit has reset, then we need
		// to perform an API request to check the new rate limit. (Give 30s of buffer for clock unsync.)
		if known && remaining < minGitHubAPIRateLimit && reset > -30*time.Second {
			bypass = true
			bypassReason = "GitHub API rate limit is exhausted"
		}
	}
	if bypass {
		remaining, reset, _, known := conn.client.RateLimit.Get()

		logArgs := []interface{}{"reason", bypassReason, "repo", args.Repo, "baseURL", conn.config.Url}
		if known {
			logArgs = append(logArgs, "rateLimitRemaining", remaining, "rateLimitReset", reset)
		} else {
			logArgs = append(logArgs, "rateLimitKnown", false)
		}

		// For public repositories, we can bypass the GitHub API and still get almost everything we
		// need (except for the repository's ID, description, and fork status).
		isPublicRepo := args.Repo != "" && conn.config.Token == ""
		if isPublicRepo {
			log15.Debug("Bypassing GitHub API when getting public repository. Some repository metadata fields will be blank.", logArgs...)

			// It's important to still check cloneability, so we don't add a bunch of junk GitHub repos that don't
			// exist (like github.com/settings/profile) or that are private and not on Sourcegraph.com.
			remoteURL := "https://" + string(args.Repo)
			if err := gitserver.DefaultClient.IsRepoCloneable(ctx, gitserver.Repo{Name: args.Repo, URL: remoteURL}); err != nil {
				return nil, true, errors.Wrap(github.ErrNotFound, fmt.Sprintf("IsRepoCloneable: %s", err))
			}

			info := githubRepoToRepoInfo(&github.Repository{URL: remoteURL}, conn)
			info.Name = args.Repo

			return info, true, nil
		}

		log15.Warn("Unable to get repository metadata from GitHub API for a (possibly) private repository.", logArgs...)
		return nil, true, ErrGitHubAPITemporarilyUnavailable
	}

	log15.Debug("GetGitHubRepository", "repo", args.Repo, "externalRepo", args.ExternalRepo)

	canUseGraphQLAPI := conn.config.Token != "" // GraphQL API requires authentication
	if canUseGraphQLAPI && args.ExternalRepo != nil && args.ExternalRepo.ServiceType == github.ServiceType {
		// Look up by external repository spec.
		ghrepo, err := conn.client.GetRepositoryByNodeID(ctx, "", args.ExternalRepo.ID)
		if ghrepo != nil {
			repo = githubRepoToRepoInfo(ghrepo, conn)
		}
		return repo, true, err
	}

	if args.Repo != "" {
		// Look up by repository name.
		nameWithOwner := strings.TrimPrefix(strings.ToLower(string(args.Repo)), conn.originalHostname+"/")
		owner, repoName, err := github.SplitRepositoryNameWithOwner(nameWithOwner)
		if err != nil {
			return nil, true, err
		}

		ghrepo, err := conn.client.GetRepository(ctx, owner, repoName)
		if ghrepo != nil {
			repo = githubRepoToRepoInfo(ghrepo, conn)
		}
		return repo, true, err
	}

	return nil, true, fmt.Errorf("unable to look up GitHub repository (%+v)", args)
}

func githubRepoToRepoInfo(ghrepo *github.Repository, conn *githubConnection) *protocol.RepoInfo {
	return &protocol.RepoInfo{
		Name:         githubRepositoryToRepoPath(conn, ghrepo),
		ExternalRepo: github.ExternalRepoSpec(ghrepo, *conn.baseURL),
		Description:  ghrepo.Description,
		Fork:         ghrepo.IsFork,
		Archived:     ghrepo.IsArchived,
		Links: &protocol.RepoLinks{
			Root:   ghrepo.URL,
			Tree:   ghrepo.URL + "/tree/{rev}/{path}",
			Blob:   ghrepo.URL + "/blob/{rev}/{path}",
			Commit: ghrepo.URL + "/commit/{commit}",
		},
		VCS: protocol.VCSInfo{
			URL: conn.authenticatedRemoteURL(ghrepo),
		},
	}
}

var gitHubRepositorySyncWorker = &worker{
	work: func(ctx context.Context, shutdown chan struct{}) {
		githubConnections := githubConnections.Get().([]*githubConnection)
		if len(githubConnections) == 0 {
			return
		}
		for _, c := range githubConnections {
			go func(c *githubConnection) {
				for {
					if rateLimitRemaining, rateLimitReset, _, ok := c.client.RateLimit.Get(); ok && rateLimitRemaining < 200 {
						wait := rateLimitReset + 10*time.Second
						log15.Warn("GitHub API rate limit is almost exhausted. Waiting until rate limit is reset.", "wait", rateLimitReset, "rateLimitRemaining", rateLimitRemaining)
						time.Sleep(wait)
					}
					updateGitHubRepositories(ctx, c)
					githubUpdateTime.WithLabelValues(c.baseURL.String()).Set(float64(time.Now().Unix()))
					select {
					case <-shutdown:
						return
					case <-time.After(GetUpdateInterval()):
					}
				}
			}(c)
		}
	},
}

// RunGitHubRepositorySyncWorker runs the worker that syncs repositories from the configured GitHub and GitHub
// Enterprise instances to Sourcegraph.
func RunGitHubRepositorySyncWorker(ctx context.Context) {
	gitHubRepositorySyncWorker.start(ctx)
}

func githubRepositoryToRepoPath(conn *githubConnection, repo *github.Repository) api.RepoName {
	return reposource.GitHubRepoName(conn.config.RepositoryPathPattern, conn.originalHostname, repo.NameWithOwner)
}

// updateGitHubRepositories ensures that all provided repositories have been added and updated on Sourcegraph.
func updateGitHubRepositories(ctx context.Context, conn *githubConnection) {
	repos, err := conn.listAllRepositories(ctx)
	if err != nil {
		log15.Error("failed to list some github repos", "error", err.Error())
	}

	repoChan := make(chan repoCreateOrUpdateRequest)
	defer close(repoChan)
	go createEnableUpdateRepos(ctx, fmt.Sprintf("github:%s", conn.config.Token), repoChan)
	for _, repo := range repos {
		// log15.Debug("github sync: create/enable/update repo", "repo", repo.NameWithOwner)
		repoChan <- repoCreateOrUpdateRequest{
			RepoCreateOrUpdateRequest: api.RepoCreateOrUpdateRequest{
				RepoName:     githubRepositoryToRepoPath(conn, repo),
				ExternalRepo: github.ExternalRepoSpec(repo, *conn.baseURL),
				Description:  repo.Description,
				Fork:         repo.IsFork,
				Archived:     repo.IsArchived,
				Enabled:      conn.config.InitialRepositoryEnablement,
			},
			URL: conn.authenticatedRemoteURL(repo),
		}
	}
}

func newGitHubConnection(config *schema.GitHubConnection, cf httpcli.Factory) (*githubConnection, error) {
	baseURL, err := url.Parse(config.Url)
	if err != nil {
		return nil, err
	}
	baseURL = NormalizeBaseURL(baseURL)
	originalHostname := baseURL.Hostname()

	apiURL, githubDotCom := github.APIRoot(baseURL)

	if cf == nil {
		cf = NewHTTPClientFactory()
	}

	var opts []httpcli.Opt
	if config.Certificate != "" {
		pool, err := newCertPool(config.Certificate)
		if err != nil {
			return nil, err
		}
		opts = append(opts, httpcli.NewCertPoolOpt(pool))
	}

	cli, err := cf.NewClient(opts...)
	if err != nil {
		return nil, err
	}

	exclude := make(map[string]bool, len(config.Exclude))
	for _, r := range config.Exclude {
		if r.Name != "" {
			exclude[strings.ToLower(r.Name)] = true
		}

		if r.Id != "" {
			exclude[r.Id] = true
		}
	}

	return &githubConnection{
		config:           config,
		exclude:          exclude,
		baseURL:          baseURL,
		githubDotCom:     githubDotCom,
		client:           github.NewClient(apiURL, config.Token, cli),
		searchClient:     github.NewClient(apiURL, config.Token, cli),
		originalHostname: originalHostname,
	}, nil
}

type githubConnection struct {
	config       *schema.GitHubConnection
	exclude      map[string]bool
	githubDotCom bool
	baseURL      *url.URL
	client       *github.Client
	// searchClient is for using the GitHub search API, which has an independent
	// rate limit much lower than non-search API requests.
	searchClient *github.Client

	// originalHostname is the hostname of config.Url (differs from client APIURL, whose host is api.github.com
	// for an originalHostname of github.com).
	originalHostname string
}

// authenticatedRemoteURL returns the repository's Git remote URL with the configured
// GitHub personal access token inserted in the URL userinfo.
func (c *githubConnection) authenticatedRemoteURL(repo *github.Repository) string {
	if c.config.GitURLType == "ssh" {
		url := fmt.Sprintf("git@%s:%s.git", c.originalHostname, repo.NameWithOwner)
		return url
	}

	if c.config.Token == "" {
		return repo.URL
	}
	u, err := url.Parse(repo.URL)
	if err != nil {
		log15.Warn("Error adding authentication to GitHub repository Git remote URL.", "url", repo.URL, "error", err)
		return repo.URL
	}
	u.User = url.User(c.config.Token)
	return u.String()
}

func (c *githubConnection) excludes(r *github.Repository) bool {
	return c.exclude[strings.ToLower(r.NameWithOwner)] || c.exclude[r.ID]
}

func (c *githubConnection) listAllRepositories(ctx context.Context) ([]*github.Repository, error) {
	set := make(map[int64]*github.Repository)
	errs := new(multierror.Error)

	for _, repositoryQuery := range c.config.RepositoryQuery {
		switch repositoryQuery {
		case "public":
			if c.githubDotCom {
				errs = multierror.Append(errs, errors.New(`unsupported configuration "public" for "repositoryQuery" for github.com`))
				continue
			}
			var sinceRepoID int64
			for {
				if err := ctx.Err(); err != nil {
					errs = multierror.Append(errs, err)
					break
				}

				repos, err := c.client.ListPublicRepositories(ctx, sinceRepoID)
				if err != nil {
					errs = multierror.Append(errs, errors.Wrapf(err, "failed to list public repositories: sinceRepoID=%d", sinceRepoID))
					break
				}
				if len(repos) == 0 {
					break
				}
				log15.Debug("github sync public", "repos", len(repos), "error", err)
				for _, r := range repos {
					set[r.DatabaseID] = r
					if sinceRepoID < r.DatabaseID {
						sinceRepoID = r.DatabaseID
					}
				}
			}
		case "affiliated":
			hasNextPage := true
			for page := 1; hasNextPage; page++ {
				if err := ctx.Err(); err != nil {
					errs = multierror.Append(errs, err)
					break
				}

				var repos []*github.Repository
				var rateLimitCost int
				var err error
				repos, hasNextPage, rateLimitCost, err = c.client.ListUserRepositories(ctx, page)
				if err != nil {
					errs = multierror.Append(errs, errors.Wrapf(err, "failed to list affiliated GitHub repositories page %d", page))
					break
				}
				rateLimitRemaining, rateLimitReset, rateLimitRetry, _ := c.client.RateLimit.Get()
				log15.Debug(
					"github sync: ListUserRepositories",
					"repos", len(repos),
					"rateLimitCost", rateLimitCost,
					"rateLimitRemaining", rateLimitRemaining,
					"rateLimitReset", rateLimitReset,
					"retryAfter", rateLimitRetry,
				)

				for _, r := range repos {
					if c.githubDotCom && r.IsFork && r.ViewerPermission == "READ" {
						log15.Debug("not syncing readonly fork", "repo", r.NameWithOwner)
						continue
					}
					set[r.DatabaseID] = r
				}

				if hasNextPage {
					time.Sleep(c.client.RateLimit.RecommendedWaitForBackgroundOp(rateLimitCost))
				}
			}

		case "none":
			// nothing to do

		default:
			// Run the query as a GitHub advanced repository search
			// (https://github.com/search/advanced).
			hasNextPage := true
			for page := 1; hasNextPage; page++ {
				if err := ctx.Err(); err != nil {
					errs = multierror.Append(errs, err)
					break
				}

				reposPage, err := c.searchClient.ListRepositoriesForSearch(ctx, repositoryQuery, page)
				if err != nil {
					errs = multierror.Append(errs, errors.Wrapf(err, "failed to list GitHub repositories for search: page=%q, searchString=%q,", page, repositoryQuery))
					break
				}

				if reposPage.TotalCount > 1000 {
					// GitHub's advanced repository search will only
					// return 1000 results. We specially handle this case
					// to ensure the admin gets a detailed error
					// message. https://github.com/sourcegraph/sourcegraph/issues/2562
					errs = multierror.Append(errs, errors.Errorf(`repositoryQuery %q would return %d results. GitHub's Search API only returns up to 1000 results. Please adjust your repository query into multiple queries such that each returns less than 1000 results. For example: {"repositoryQuery": %s}`, repositoryQuery, reposPage.TotalCount, exampleRepositoryQuerySplit(repositoryQuery)))
					break
				}

				hasNextPage = reposPage.HasNextPage
				repos := reposPage.Repos

				rateLimitRemaining, rateLimitReset, rateLimitRetry, _ := c.searchClient.RateLimit.Get()
				log15.Debug(
					"github sync: ListRepositoriesForSearch",
					"searchString", repositoryQuery,
					"repos", len(repos),
					"rateLimitRemaining", rateLimitRemaining,
					"rateLimitReset", rateLimitReset,
					"retryAfter", rateLimitRetry,
				)

				for _, r := range repos {
					set[r.DatabaseID] = r
				}

				if hasNextPage {
					// GitHub search has vastly different rate limits to
					// the normal GitHub API (30req/m vs
					// 5000req/h). RecommendedWaitForBackgroundOp has
					// heuristics tuned for the normal API, part of which
					// is to not sleep if we have ample rate limit left.
					//
					// So we only let the heuristic kick in if we have
					// less than 5 requests left.
					remaining, _, retryAfter, ok := c.searchClient.RateLimit.Get()
					if retryAfter > 0 || (ok && remaining < 5) {
						time.Sleep(c.searchClient.RateLimit.RecommendedWaitForBackgroundOp(1))
					}
				}
			}
		}
	}

	for _, nameWithOwner := range c.config.Repos {
		if err := ctx.Err(); err != nil {
			errs = multierror.Append(errs, err)
			break
		}

		owner, name, err := github.SplitRepositoryNameWithOwner(nameWithOwner)
		if err != nil {
			errs = multierror.Append(errs, errors.New("Invalid GitHub repository: nameWithOwner="+nameWithOwner))
			break
		}
		repo, err := c.client.GetRepository(ctx, owner, name)
		if err != nil {
			// TODO(tsenart): When implementing dry-run, reconsider alternatives to return
			// 404 errors on external service config validation.
			if github.IsNotFound(err) {
				log15.Warn("skipping missing github.repos entry:", "name", nameWithOwner, "err", err)
				continue
			}
			errs = multierror.Append(errs, errors.Wrapf(err, "Error getting GitHub repository: nameWithOwner=%s", nameWithOwner))
			break
		}
		log15.Debug("github sync: GetRepository", "repo", repo.NameWithOwner)
		set[repo.DatabaseID] = repo
		time.Sleep(c.client.RateLimit.RecommendedWaitForBackgroundOp(1)) // 0-duration sleep unless nearing rate limit exhaustion
	}

	repos := make([]*github.Repository, 0, len(set))
	for _, repo := range set {
		if !c.excludes(repo) {
			repos = append(repos, repo)
		}
	}

	return repos, errs.ErrorOrNil()
}

func exampleRepositoryQuerySplit(q string) string {
	var qs []string
	for _, suffix := range []string{"created:>=2019", "created:2018", "created:2016..2017", "created:<2016"} {
		qs = append(qs, fmt.Sprintf("%s %s", q, suffix))
	}
	// Avoid escaping < and >
	var b bytes.Buffer
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	_ = enc.Encode(qs)
	return strings.TrimSpace(b.String())
}
