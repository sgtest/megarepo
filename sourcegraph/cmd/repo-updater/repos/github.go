package repos

import (
	"context"
	"fmt"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/internal/externalservice/github"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/atomicvalue"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/conf/reposource"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// GitHubServiceType is the (api.ExternalRepoSpec).ServiceType value for GitHub repositories. The ServiceID value
// is the base URL to the GitHub instance (https://github.com or the GitHub Enterprise URL).
const GitHubServiceType = "github"

// GitHubExternalRepoSpec returns an api.ExternalRepoSpec that refers to the specified GitHub repository.
func GitHubExternalRepoSpec(repo *github.Repository, baseURL url.URL) *api.ExternalRepoSpec {
	return &api.ExternalRepoSpec{
		ID:          repo.ID,
		ServiceType: GitHubServiceType,
		ServiceID:   NormalizeBaseURL(&baseURL).String(),
	}
}

var githubConnections = atomicvalue.New()

func init() {
	conf.Watch(func() {
		githubConnections.Set(func() interface{} {
			githubConf := conf.Get().Github

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

			var conns []*githubConnection
			for _, c := range githubConf {
				conn, err := newGitHubConnection(c)
				if err != nil {
					log15.Error("Error processing configured GitHub connection. Skipping it.", "url", c.Url, "error", err)
					continue
				}
				conns = append(conns, conn)
			}
			return conns
		})
		gitHubRepositorySyncWorker.restart()
	})
}

// getGitHubConnection returns the GitHub connection (config + API client) that is responsible for
// the repository specified by the args.
func getGitHubConnection(args protocol.RepoLookupArgs) (*githubConnection, error) {
	githubConnections := githubConnections.Get().([]*githubConnection)
	if args.ExternalRepo != nil && args.ExternalRepo.ServiceType == GitHubServiceType {
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
		// Look up by repository URI.
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

// GetGitHubRepository queries a configured GitHub connection endpoint for information about the
// specified repository.
//
// If args.Repo refers to a repository that is not known to be on a configured GitHub connection's
// host, it returns authoritative == false.
func GetGitHubRepository(ctx context.Context, args protocol.RepoLookupArgs) (repo *protocol.RepoInfo, authoritative bool, err error) {
	if GetGitHubRepositoryMock != nil {
		return GetGitHubRepositoryMock(args)
	}

	ghrepoToRepoInfo := func(ghrepo *github.Repository, conn *githubConnection) *protocol.RepoInfo {
		return &protocol.RepoInfo{
			URI:          githubRepositoryToRepoPath(conn, ghrepo),
			ExternalRepo: GitHubExternalRepoSpec(ghrepo, *conn.baseURL),
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
		remaining, reset, known := conn.client.RateLimit.Get()
		// If we're below the min rate limit, bypass the GitHub API. But if the rate limit has reset, then we need
		// to perform an API request to check the new rate limit. (Give 30s of buffer for clock unsync.)
		if known && remaining < minGitHubAPIRateLimit && reset > -30*time.Second {
			bypass = true
			bypassReason = "GitHub API rate limit is exhausted"
		}
	}
	if bypass {
		remaining, reset, known := conn.client.RateLimit.Get()

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

			return &protocol.RepoInfo{
				URI:          args.Repo,
				ExternalRepo: nil,
				Description:  "",
				Fork:         false,
				Archived:     false,
				Links: &protocol.RepoLinks{
					Root:   remoteURL,
					Tree:   remoteURL + "/tree/{rev}/{path}",
					Blob:   remoteURL + "/blob/{rev}/{path}",
					Commit: remoteURL + "/commit/{commit}",
				},
				VCS: protocol.VCSInfo{URL: remoteURL},
			}, true, nil
		}

		log15.Warn("Unable to get repository metadata from GitHub API for a (possibly) private repository.", logArgs...)
		return nil, true, ErrGitHubAPITemporarilyUnavailable
	}

	log15.Debug("GetGitHubRepository", "repo", args.Repo, "externalRepo", args.ExternalRepo)

	canUseGraphQLAPI := conn.config.Token != "" // GraphQL API requires authentication
	if canUseGraphQLAPI && args.ExternalRepo != nil && args.ExternalRepo.ServiceType == GitHubServiceType {
		// Look up by external repository spec.
		ghrepo, err := conn.client.GetRepositoryByNodeID(ctx, args.ExternalRepo.ID)
		if ghrepo != nil {
			repo = ghrepoToRepoInfo(ghrepo, conn)
		}
		return repo, true, err
	}

	if args.Repo != "" {
		// Look up by repository URI.
		nameWithOwner := strings.TrimPrefix(strings.ToLower(string(args.Repo)), conn.originalHostname+"/")
		owner, repoName, err := github.SplitRepositoryNameWithOwner(nameWithOwner)
		if err != nil {
			return nil, true, err
		}

		ghrepo, err := conn.client.GetRepository(ctx, owner, repoName)
		if ghrepo != nil {
			repo = ghrepoToRepoInfo(ghrepo, conn)
		}
		return repo, true, err
	}

	return nil, true, fmt.Errorf("unable to look up GitHub repository (%+v)", args)
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
					if rateLimitRemaining, rateLimitReset, ok := c.client.RateLimit.Get(); ok && rateLimitRemaining < 200 {
						wait := rateLimitReset + 10*time.Second
						log15.Warn("GitHub API rate limit is almost exhausted. Waiting until rate limit is reset.", "wait", rateLimitReset, "rateLimitRemaining", rateLimitRemaining)
						time.Sleep(wait)
					}
					updateGitHubRepositories(ctx, c)
					githubUpdateTime.WithLabelValues(c.baseURL.String()).Set(float64(time.Now().Unix()))
					select {
					case <-shutdown:
						return
					case <-time.After(getUpdateInterval()):
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

func githubRepositoryToRepoPath(conn *githubConnection, repo *github.Repository) api.RepoURI {
	return reposource.GitHubRepoURI(conn.config.RepositoryPathPattern, conn.originalHostname, repo.NameWithOwner)
}

// updateGitHubRepositories ensures that all provided repositories have been added and updated on Sourcegraph.
func updateGitHubRepositories(ctx context.Context, conn *githubConnection) {
	repos := conn.listAllRepositories(ctx)

	repoChan := make(chan repoCreateOrUpdateRequest)
	defer close(repoChan)
	go createEnableUpdateRepos(ctx, fmt.Sprintf("github:%s", conn.config.Token), repoChan)
	for repo := range repos {
		// log15.Debug("github sync: create/enable/update repo", "repo", repo.NameWithOwner)
		repoChan <- repoCreateOrUpdateRequest{
			RepoCreateOrUpdateRequest: api.RepoCreateOrUpdateRequest{
				RepoURI:      githubRepositoryToRepoPath(conn, repo),
				ExternalRepo: GitHubExternalRepoSpec(repo, *conn.baseURL),
				Description:  repo.Description,
				Fork:         repo.IsFork,
				Archived:     repo.IsArchived,
				Enabled:      conn.config.InitialRepositoryEnablement,
			},
			URL: conn.authenticatedRemoteURL(repo),
		}
	}
}

func newGitHubConnection(config *schema.GitHubConnection) (*githubConnection, error) {
	baseURL, err := url.Parse(config.Url)
	if err != nil {
		return nil, err
	}
	baseURL = NormalizeBaseURL(baseURL)
	originalHostname := baseURL.Hostname()

	// GitHub.com's API is hosted on api.github.com.
	apiURL := *baseURL
	githubDotCom := false
	if hostname := strings.ToLower(apiURL.Hostname()); hostname == "github.com" || hostname == "www.github.com" {
		// GitHub.com
		apiURL = url.URL{Scheme: "https", Host: "api.github.com", Path: "/"}
		githubDotCom = true
	} else {
		// GitHub Enterprise
		if apiURL.Path == "" || apiURL.Path == "/" {
			apiURL = *apiURL.ResolveReference(&url.URL{Path: "/api"})
		}
	}

	transport, err := cachedTransportWithCertTrusted(config.Certificate)
	if err != nil {
		return nil, err
	}

	// Create a shared repository cache for client and searchClient, since they
	// have independent rate limits but are querying the same underlying data.
	repoCache := github.NewRepoCache(&apiURL, config.Token)

	return &githubConnection{
		config:           config,
		baseURL:          baseURL,
		githubDotCom:     githubDotCom,
		client:           github.NewClient(&apiURL, config.Token, transport, repoCache),
		searchClient:     github.NewClient(&apiURL, config.Token, transport, repoCache),
		originalHostname: originalHostname,
	}, nil
}

type githubConnection struct {
	config       *schema.GitHubConnection
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

func (c *githubConnection) listAllRepositories(ctx context.Context) <-chan *github.Repository {
	const first = 100 // max GitHub API "first" parameter
	ch := make(chan *github.Repository, first)

	var wg sync.WaitGroup

	if len(c.config.RepositoryQuery) == 0 {
		// Users need to specify ["none"] to disable mirroring.
		if c.githubDotCom {
			// Doesn't make sense to try to enumerate all public repos on github.com
			c.config.RepositoryQuery = []string{"affiliated"}
		} else {
			c.config.RepositoryQuery = []string{"public", "affiliated"}
		}
	}
	for _, repositoryQuery := range c.config.RepositoryQuery {
		wg.Add(1)
		go func(repositoryQuery string) {
			defer wg.Done()
			switch repositoryQuery {
			case "public":
				if c.githubDotCom {
					log15.Warn(`ignoring unsupported configuration "public" for "repositoryQuery" for github.com`)
					return
				}
				var sinceRepoID int64
				for {
					repos, err := c.client.ListPublicRepositories(ctx, sinceRepoID)
					if err != nil {
						log15.Error("Error listing public repositories", "sinceRepoID", sinceRepoID, "error", err)
						return
					}
					if len(repos) == 0 {
						// Last page
						return
					}
					for _, r := range repos {
						ch <- r
						if sinceRepoID < r.DatabaseID {
							sinceRepoID = r.DatabaseID
						}
					}
				}
			case "affiliated":
				hasNextPage := true
				for page := 1; hasNextPage; page++ {
					var repos []*github.Repository
					var rateLimitCost int
					var err error
					repos, hasNextPage, rateLimitCost, err = c.client.ListViewerRepositories(ctx, page)
					if err != nil {
						log15.Error("Error listing viewer's affiliated GitHub repositories", "page", page, "error", err)
						break
					}
					rateLimitRemaining, rateLimitReset, _ := c.client.RateLimit.Get()
					log15.Debug("github sync: ListViewerRepositories", "repos", len(repos), "rateLimitCost", rateLimitCost, "rateLimitRemaining", rateLimitRemaining, "rateLimitReset", rateLimitReset)
					for _, r := range repos {
						if c.githubDotCom && r.IsFork && r.ViewerPermission == "READ" {
							log15.Debug("not syncing readonly fork", "repo", r.NameWithOwner)
							continue
						}
						// log15.Debug("github sync: ListViewerRepositories: repo", "repo", r.NameWithOwner)
						ch <- r
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
					var repos []*github.Repository
					var rateLimitCost int
					var err error
					repos, hasNextPage, rateLimitCost, err = c.searchClient.ListRepositoriesForSearch(ctx, repositoryQuery, page)
					if err != nil {
						log15.Error("Error listing GitHub repositories for search", "searchString", repositoryQuery, "page", page, "error", err)
						break
					}
					rateLimitRemaining, rateLimitReset, _ := c.searchClient.RateLimit.Get()
					log15.Debug("github sync: ListRepositoriesForSearch", "searchString", repositoryQuery, "repos", len(repos), "rateLimitCost", rateLimitCost, "rateLimitRemaining", rateLimitRemaining, "rateLimitReset", rateLimitReset)
					for _, r := range repos {
						ch <- r
					}
					if hasNextPage {
						time.Sleep(c.searchClient.RateLimit.RecommendedWaitForBackgroundOp(rateLimitCost))
					}
				}
			}
		}(repositoryQuery)
	}

	wg.Add(1)
	go func() {
		defer wg.Done()
		for _, nameWithOwner := range c.config.Repos {
			owner, name, err := github.SplitRepositoryNameWithOwner(nameWithOwner)
			if err != nil {
				log15.Error("Invalid GitHub repository", "nameWithOwner", nameWithOwner)
				continue
			}
			repo, err := c.client.GetRepository(ctx, owner, name)
			if err != nil {
				log15.Error("Error getting GitHub repository", "nameWithOwner", nameWithOwner, "error", err)
				continue
			}
			log15.Debug("github sync: GetRepository", "repo", repo.NameWithOwner)
			ch <- repo
			time.Sleep(c.client.RateLimit.RecommendedWaitForBackgroundOp(1)) // 0-duration sleep unless nearing rate limit exhaustion
		}
	}()

	go func() {
		wg.Wait()
		close(ch)
	}()

	return unique(ch)
}

// unique returns a channel that only forwards repositories
// that have never been sent on the channel before.
func unique(in <-chan *github.Repository) <-chan *github.Repository {
	out := make(chan *github.Repository)
	go func() {
		found := make(map[string]struct{})
		for repo := range in {
			if _, ok := found[repo.URL]; !ok {
				out <- repo
				found[repo.URL] = struct{}{}
			}
		}
		close(out)
	}()
	return out
}
