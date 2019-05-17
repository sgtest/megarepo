package github

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"net/url"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/github"
	"github.com/sourcegraph/sourcegraph/pkg/rcache"
)

// Provider implements authz.Provider for GitHub repository permissions.
type Provider struct {
	client   *github.Client
	codeHost *github.CodeHost
	cacheTTL time.Duration
	cache    cache
}

func NewProvider(githubURL *url.URL, baseToken string, cacheTTL time.Duration, mockCache cache) *Provider {
	apiURL, _ := github.APIRoot(githubURL)
	client := github.NewClient(apiURL, baseToken, nil)

	p := &Provider{
		codeHost: github.NewCodeHost(githubURL),
		client:   client,
		cache:    mockCache,
		cacheTTL: cacheTTL,
	}
	// Note: this will use the same underlying Redis instance and key namespace for every instance
	// of Provider.  This is by design, so that different instances, even in different processes,
	// will share cache entries.
	if p.cache == nil {
		p.cache = rcache.NewWithTTL(fmt.Sprintf("githubAuthz:%s", githubURL.String()), int(math.Ceil(cacheTTL.Seconds())))
	}
	return p
}

var _ authz.Provider = ((*Provider)(nil))

// Repos implements the authz.Provider interface.
func (p *Provider) Repos(ctx context.Context, repos map[authz.Repo]struct{}) (mine map[authz.Repo]struct{}, others map[authz.Repo]struct{}) {
	return authz.GetCodeHostRepos(p.codeHost, repos)
}

// RepoPerms implements the authz.Provider interface.
//
// It computes permissions by keeping track of two classes of info:
// * Whether a given user can access a given repository
// * Whether a given repository is public
//
// For each repo in the input set, we look first to see if the above information is cached in Redis.
// If not, then the info is computed by querying the GitHub API. A separate query is issued for each
// repository (and for each user for the explicit case).
func (p *Provider) RepoPerms(ctx context.Context, userAccount *extsvc.ExternalAccount, repos map[authz.Repo]struct{}) (map[api.RepoName]map[authz.Perm]bool, error) {
	remaining, _ := p.Repos(ctx, repos)
	remainingPublic := remaining
	if len(remaining) == 0 {
		return nil, nil
	}

	perms := map[api.RepoName]map[authz.Perm]bool{}
	populatePermsPublic := func(checkAccess func(ctx context.Context, repos map[authz.Repo]struct{}) (map[string]bool, error)) error {
		nextRemaining := map[authz.Repo]struct{}{}
		nextRemainingPublic := map[authz.Repo]struct{}{}
		canAccess, err := checkAccess(ctx, remainingPublic)
		if err != nil {
			return err
		}
		for repo := range remaining {
			canAcc, isExplicit := canAccess[repo.ExternalRepoSpec.ID]
			if canAcc {
				perms[repo.RepoName] = map[authz.Perm]bool{authz.Read: true}
				continue
			}
			nextRemaining[repo] = struct{}{}
			if !isExplicit {
				nextRemainingPublic[repo] = struct{}{}
			}
		}
		remaining = nextRemaining
		remainingPublic = nextRemainingPublic
		return nil
	}
	populatePerms := func(checkAccess func(ctx context.Context, userAccount *extsvc.ExternalAccount, repos map[authz.Repo]struct{}) (map[string]bool, error)) error {
		nextRemaining := map[authz.Repo]struct{}{}
		canAccess, err := checkAccess(ctx, userAccount, remaining)
		if err != nil {
			return err
		}
		for repo := range remaining {
			if canAcc, isExplicit := canAccess[repo.ExternalRepoSpec.ID]; isExplicit {
				perms[repo.RepoName] = map[authz.Perm]bool{authz.Read: canAcc}
				continue
			}
			nextRemaining[repo] = struct{}{}
		}
		remaining = nextRemaining
		return nil
	}

	if err := populatePerms(p.getCachedUserRepos); err != nil {
		return nil, err
	}
	if len(remaining) == 0 {
		return perms, nil
	}
	if err := populatePermsPublic(p.getCachedPublicRepos); err != nil {
		return nil, err
	}
	if len(remaining) == 0 {
		return perms, nil
	}
	if err := populatePerms(p.fetchAndSetUserRepos); err != nil {
		return nil, err
	}
	if len(remaining) == 0 {
		return perms, nil
	}
	if err := populatePermsPublic(p.fetchAndSetPublicRepos); err != nil {
		return nil, err
	}
	return perms, nil
}

// fetchAndSetPublicRepos accepts a set of repositories and returns a map from repository external
// ID (the GitHub repository GraphQL ID) to true/false indicating whether the repository is public
// or private. It consults and updates the cache. As a side effect, it caches the publicness of the
// repos.
func (p *Provider) fetchAndSetPublicRepos(ctx context.Context, repos map[authz.Repo]struct{}) (map[string]bool, error) {
	isPublic, err := p.fetchPublicRepos(ctx, repos)
	if err != nil {
		return nil, err
	}
	if err := p.setCachedPublicRepos(ctx, isPublic); err != nil {
		return nil, err
	}
	return isPublic, nil
}

// setCachedPublicRepos updates the cache with a map from GitHub repo ID to true/false indicating
// whether the repo is public or private. The GitHub repo ID is the GraphQL API ID ("repository node
// ID").
//
// Internally, it sets a separate cache key for each repo ID.
func (p *Provider) setCachedPublicRepos(ctx context.Context, isPublic map[string]bool) error {
	setArgs := make([][2]string, 0, len(isPublic))
	for k, v := range isPublic {
		key := publicRepoCacheKey(k)
		val, err := json.Marshal(publicRepoCacheVal{
			Public: v,
			TTL:    p.cacheTTL,
		})
		if err != nil {
			return err
		}
		setArgs = append(setArgs, [2]string{key, string(val)})
	}
	p.cache.SetMulti(setArgs...)
	return nil
}

// getCachedPublicRepos accepts a set of repos and returns a map from repo ID to true/false
// indicating whether the repo is public or private. The returned map may be incomplete (i.e., not
// every input repo may be represented in the key set) due to cache incompleteness.
func (p *Provider) getCachedPublicRepos(ctx context.Context, repos map[authz.Repo]struct{}) (isPublic map[string]bool, err error) {
	if len(repos) == 0 {
		return nil, nil
	}
	isPublic = make(map[string]bool)
	repoList := make([]string, 0, len(repos))
	getArgs := make([]string, 0, len(repos))
	for r := range repos {
		getArgs = append(getArgs, publicRepoCacheKey(r.ExternalRepoSpec.ID))
		repoList = append(repoList, r.ExternalRepoSpec.ID)
	}
	vals := p.cache.GetMulti(getArgs...)
	for i, v := range vals {
		if len(v) == 0 {
			continue
		}
		var val publicRepoCacheVal
		if err := json.Unmarshal(v, &val); err != nil {
			return nil, err
		}
		if p.cacheTTL < val.TTL {
			// if the cache TTL is now less than the cache entry TTL, invalidate that entry
			continue
		}
		isPublic[repoList[i]] = val.Public
	}
	return isPublic, nil
}

// fetchPublicRepos returns a map from GitHub repository ID (the GraphQL repo node ID) to true/false
// indicating whether a repository is public (true) or private (false).
func (p *Provider) fetchPublicRepos(ctx context.Context, repos map[authz.Repo]struct{}) (map[string]bool, error) {
	isPublic := make(map[string]bool)
	for repo := range repos {
		ghRepo, err := p.client.GetRepositoryByNodeID(ctx, "", repo.ExternalRepoSpec.ID)
		if err == github.ErrNotFound {
			// Note: we could set `isPublic[repo.ExternalRepoSpec.ID] = false` here, but
			// purposefully don't cache if a repo is private in case it later becomes public.
			continue
		}
		if err != nil {
			return nil, err
		}
		isPublic[repo.ExternalRepoSpec.ID] = !ghRepo.IsPrivate
	}
	return isPublic, nil
}

// fetchAndSetUserRepos accepts a user account and a set of repos. It returns a map from repository
// external ID to true/false indicating whether the given user has read access to each repo. If a
// repo ID is missing from the return map, the user does not have read access to that repo. As a
// side effect, it caches the fetched repos (whether the given user has access to each and whether
// each is public).
func (p *Provider) fetchAndSetUserRepos(ctx context.Context, userAccount *extsvc.ExternalAccount, repos map[authz.Repo]struct{}) (isAllowed map[string]bool, err error) {
	if userAccount == nil {
		return nil, nil
	}

	repoIDs := make([]string, len(repos))
	i := 0
	for repo := range repos {
		repoIDs[i] = repo.ID
		i++
	}

	canAccess, isPublic, err := p.fetchUserRepos(ctx, userAccount, repoIDs)
	if err != nil {
		return nil, err
	}
	userRepos := make(map[string]bool)
	publicRepos := make(map[string]bool)
	for r := range repos {
		userRepos[r.ExternalRepoSpec.ID] = canAccess[r.ExternalRepoSpec.ID]
		publicRepos[r.ExternalRepoSpec.ID] = isPublic[r.ExternalRepoSpec.ID]
	}

	if err := p.setCachedUserRepos(ctx, userAccount, userRepos); err != nil {
		return nil, err
	}
	if err := p.setCachedPublicRepos(ctx, publicRepos); err != nil { // also cache whether repos are public
		return nil, err
	}
	return userRepos, nil
}

// setCachedUserRepos updates the cache with a map from GitHub repo ID to true/false indicating
// whether the user can access the repo. The GitHub repo ID is the GraphQL API ID ("repository node
// ID").
//
// Internally, it sets a separate cache key for each user and repo ID.
func (p *Provider) setCachedUserRepos(ctx context.Context, userAccount *extsvc.ExternalAccount, isAllowed map[string]bool) error {
	setArgs := make([][2]string, 0, len(isAllowed))
	for k, v := range isAllowed {
		rkey, err := json.Marshal(userRepoCacheKey{User: userAccount.AccountID, Repo: k})
		if err != nil {
			return err
		}
		rval, err := json.Marshal(userRepoCacheVal{Read: v, TTL: p.cacheTTL})
		if err != nil {
			return err
		}
		setArgs = append(setArgs, [2]string{string(rkey), string(rval)})
	}
	p.cache.SetMulti(setArgs...)
	return nil
}

// getCachedUserRepos accepts a user account and set of repos and returns a map from repo ID to
// true/false indicating whether the user can access the repo. The returned map may be incomplete
// (i.e., not every input repo may be represented in the key set) due to cache incompleteness.
func (p *Provider) getCachedUserRepos(ctx context.Context, userAccount *extsvc.ExternalAccount, repos map[authz.Repo]struct{}) (cachedUserRepos map[string]bool, err error) {
	if userAccount == nil {
		return nil, nil
	}

	getArgs := make([]string, 0, len(repos))
	repoList := make([]string, 0, len(repos))
	for repo := range repos {
		rkey, err := json.Marshal(userRepoCacheKey{
			User: userAccount.AccountID,
			Repo: repo.ExternalRepoSpec.ID,
		})
		if err != nil {
			return nil, err
		}
		getArgs = append(getArgs, string(rkey))
		repoList = append(repoList, repo.ExternalRepoSpec.ID)
	}

	cacheVals := p.cache.GetMulti(getArgs...)
	if len(cacheVals) == 0 {
		return nil, nil
	}
	cachedIsAllowed := make(map[string]bool)
	for i, v := range cacheVals {
		if len(v) == 0 {
			continue
		}

		var val userRepoCacheVal
		if err := json.Unmarshal(v, &val); err != nil {
			return nil, err
		}
		if p.cacheTTL < val.TTL {
			// if the cache TTL is now less than the cache entry TTL, invalidate that entry
			continue
		}
		cachedIsAllowed[repoList[i]] = val.Read
	}
	return cachedIsAllowed, nil
}

func (p *Provider) fetchUserRepos(ctx context.Context, userAccount *extsvc.ExternalAccount, repoIDs []string) (canAccess map[string]bool, isPublic map[string]bool, err error) {
	_, tok, err := github.GetExternalAccountData(&userAccount.ExternalAccountData)
	if err != nil {
		return nil, nil, err
	}

	// Batch fetch repos from API
	ghRepos := make(map[string]*github.Repository)
	for i := 0; i < len(repoIDs); i += github.MaxNodeIDs {
		j := i + github.MaxNodeIDs
		if j > len(repoIDs) {
			j = len(repoIDs)
		}
		ghReposBatch, err := p.client.GetRepositoriesByNodeIDFromAPI(ctx, tok.AccessToken, repoIDs[i:j])
		if err != nil {
			return nil, nil, err
		}
		for k, v := range ghReposBatch {
			ghRepos[k] = v
		}
	}

	isPublic = make(map[string]bool)
	for _, r := range ghRepos {
		isPublic[r.ID] = !r.IsPrivate
	}
	canAccess = make(map[string]bool)
	for _, rid := range repoIDs {
		_, exists := ghRepos[rid]
		canAccess[rid] = exists
	}

	return canAccess, isPublic, nil
}

// fetchUserRepo fetches whether the given user can access the given repo from the GitHub API.
func (p *Provider) fetchUserRepo(ctx context.Context, userAccount *extsvc.ExternalAccount, repoID string) (canAccess bool, isPublic bool, err error) {
	_, tok, err := github.GetExternalAccountData(&userAccount.ExternalAccountData)
	if err != nil {
		return false, false, err
	}
	ghRepo, err := p.client.GetRepositoryByNodeIDNoCache(ctx, tok.AccessToken, repoID)
	if err != nil {
		if err == github.ErrNotFound {
			return false, false, nil
		}
		return false, false, err
	}
	return true, !ghRepo.IsPrivate, nil
}

// FetchAccount implements the authz.Provider interface. It always returns nil, because the GitHub
// API doesn't currently provide a way to fetch user by external SSO account.
func (p *Provider) FetchAccount(ctx context.Context, user *types.User, current []*extsvc.ExternalAccount) (mine *extsvc.ExternalAccount, err error) {
	return nil, nil
}

func (p *Provider) ServiceID() string {
	return p.codeHost.ServiceID()
}

func (p *Provider) ServiceType() string {
	return p.codeHost.ServiceType()
}

func (p *Provider) Validate() (problems []string) {
	return nil
}
