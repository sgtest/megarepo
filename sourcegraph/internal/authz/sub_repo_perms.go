package authz

import (
	"context"
	"io/fs"
	"strconv"
	"strings"
	"time"

	"github.com/gobwas/glob"
	lru "github.com/hashicorp/golang-lru"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"golang.org/x/sync/singleflight"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// RepoContent specifies data existing in a repo. It currently only supports
// paths but will be extended in future to support other pieces of metadata, for
// example branch.
type RepoContent struct {
	Repo api.RepoName
	Path string
}

// SubRepoPermissionChecker is the interface exposed by the SubRepoPermsClient and is
// exposed to allow consumers to mock out the client.
type SubRepoPermissionChecker interface {
	// Permissions returns the level of access the provided user has for the requested
	// content.
	//
	// If the userID represents an anonymous user, ErrUnauthenticated is returned.
	Permissions(ctx context.Context, userID int32, content RepoContent) (Perms, error)

	// Enabled indicates whether sub-repo permissions are enabled.
	Enabled() bool

	// EnabledForRepoId indicates whether sub-repo permissions are enabled for the given repoID
	EnabledForRepoId(ctx context.Context, repoId api.RepoID) (bool, error)

	// EnabledForRepo indicates whether sub-repo permissions are enabled for the given repo
	EnabledForRepo(ctx context.Context, repo api.RepoName) (bool, error)
}

// DefaultSubRepoPermsChecker allows us to use a single instance with a shared
// cache and database connection. Since we don't have a database connection at
// initialisation time, services that require this client should initialise it in
// their main function.
var DefaultSubRepoPermsChecker SubRepoPermissionChecker = &noopPermsChecker{}

type noopPermsChecker struct{}

func (*noopPermsChecker) Permissions(ctx context.Context, userID int32, content RepoContent) (Perms, error) {
	return None, nil
}

func (*noopPermsChecker) Enabled() bool {
	return false
}

func (*noopPermsChecker) EnabledForRepoId(ctx context.Context, repoId api.RepoID) (bool, error) {
	return false, nil
}

func (*noopPermsChecker) EnabledForRepo(ctx context.Context, repo api.RepoName) (bool, error) {
	return false, nil
}

var _ SubRepoPermissionChecker = &SubRepoPermsClient{}

// SubRepoPermissionsGetter allows getting sub repository permissions.
type SubRepoPermissionsGetter interface {
	// GetByUser returns the known sub repository permissions rules known for a user.
	GetByUser(ctx context.Context, userID int32) (map[api.RepoName]SubRepoPermissions, error)

	// RepoIdSupported returns true if repo with the given ID has sub-repo permissions
	RepoIdSupported(ctx context.Context, repoId api.RepoID) (bool, error)

	// RepoSupported returns true if repo with the given name has sub-repo permissions
	RepoSupported(ctx context.Context, repo api.RepoName) (bool, error)
}

// SubRepoPermsClient is a concrete implementation of SubRepoPermissionChecker.
// Always use NewSubRepoPermsClient to instantiate an instance.
type SubRepoPermsClient struct {
	permissionsGetter SubRepoPermissionsGetter
	clock             func() time.Time
	since             func(time.Time) time.Duration

	group *singleflight.Group
	cache *lru.Cache
}

const defaultCacheSize = 1000
const defaultCacheTTL = 10 * time.Second

// cachedRules caches the perms rules known for a particular user by repo.
type cachedRules struct {
	rules     map[api.RepoName]compiledRules
	timestamp time.Time
}

type compiledRules struct {
	includes    []glob.Glob
	excludes    []glob.Glob
	dirIncludes []glob.Glob
}

// NewSubRepoPermsClient instantiates an instance of authz.SubRepoPermsClient
// which implements SubRepoPermissionChecker.
//
// SubRepoPermissionChecker is responsible for checking whether a user has access
// to data within a repo. Sub-repository permissions enforcement is on top of
// existing repository permissions, which means the user must already have access
// to the repository itself. The intention is for this client to be created once
// at startup and passed in to all places that need to check sub repo
// permissions.
//
// Note that sub-repo permissions are currently opt-in via the
// experimentalFeatures.enableSubRepoPermissions option.
func NewSubRepoPermsClient(permissionsGetter SubRepoPermissionsGetter) (*SubRepoPermsClient, error) {
	cache, err := lru.New(defaultCacheSize)
	if err != nil {
		return nil, errors.Wrap(err, "creating LRU cache")
	}

	conf.Watch(func() {
		if c := conf.Get(); c.ExperimentalFeatures != nil && c.ExperimentalFeatures.SubRepoPermissions != nil && c.ExperimentalFeatures.SubRepoPermissions.UserCacheSize > 0 {
			cache.Resize(c.ExperimentalFeatures.SubRepoPermissions.UserCacheSize)
		}
	})

	return &SubRepoPermsClient{
		permissionsGetter: permissionsGetter,
		clock:             time.Now,
		since:             time.Since,
		group:             &singleflight.Group{},
		cache:             cache,
	}, nil
}

// WithGetter returns a new instance that uses the supplied getter. The cache
// from the original instance is left intact.
func (s *SubRepoPermsClient) WithGetter(g SubRepoPermissionsGetter) *SubRepoPermsClient {
	return &SubRepoPermsClient{
		permissionsGetter: g,
		clock:             s.clock,
		since:             s.since,
		group:             s.group,
		cache:             s.cache,
	}
}

// subRepoPermsPermissionsDuration tracks the behaviour and performance of Permissions()
var subRepoPermsPermissionsDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
	Name: "authz_sub_repo_perms_permissions_duration_seconds",
	Help: "Time spent syncing",
}, []string{"error"})

// subRepoPermsCacheHit tracks the number of cache hits and misses for sub-repo permissions
var subRepoPermsCacheHit = promauto.NewCounterVec(prometheus.CounterOpts{
	Name: "authz_sub_repo_perms_permissions_cache_count",
	Help: "The number of sub-repo perms cache hits or misses",
}, []string{"hit"})

// Permissions return the current permissions granted to the given user on the
// given content. If sub-repo permissions are disabled, it is a no-op that return
// Read.
func (s *SubRepoPermsClient) Permissions(ctx context.Context, userID int32, content RepoContent) (perms Perms, err error) {
	// Are sub-repo permissions enabled at the site level
	if !s.Enabled() {
		return Read, nil
	}

	began := time.Now()
	defer func() {
		took := time.Since(began).Seconds()
		subRepoPermsPermissionsDuration.WithLabelValues(strconv.FormatBool(err != nil)).Observe(took)
	}()

	if s.permissionsGetter == nil {
		return None, errors.New("PermissionsGetter is nil")
	}

	if userID == 0 {
		return None, &ErrUnauthenticated{}
	}

	// An empty path is equivalent to repo permissions so we can assume it has
	// already been checked at that level.
	if content.Path == "" {
		return Read, nil
	}

	repoRules, err := s.getCompiledRules(ctx, userID)
	if err != nil {
		return None, errors.Wrap(err, "compiling match rules")
	}

	rules, ok := repoRules[content.Repo]
	if !ok {
		// If we make it this far it implies that we have access at the repo level.
		// Having any empty set of rules here implies that we can access the whole repo.
		// Repos that support sub-repo permissions will only have an entry in our
		// repo_permissions table if after all sub-repo permissions have been processed.
		return Read, nil
	}

	// The current path needs to either be included or NOT excluded and we'll give
	// preference to exclusion.
	for _, rule := range rules.excludes {
		if rule.Match(content.Path) {
			return None, nil
		}
	}
	for _, rule := range rules.includes {
		if rule.Match(content.Path) {
			return Read, nil
		}
	}

	// We also want to match any directories above paths that we include so that we
	// can browse down the file hierarchy.
	if strings.HasSuffix(content.Path, "/") {
		for _, rule := range rules.dirIncludes {
			if rule.Match(content.Path) {
				return Read, nil
			}
		}
	}

	// Return None if no rule matches to be safe
	return None, nil
}

// getCompiledRules fetches rules for the given repo with caching.
func (s *SubRepoPermsClient) getCompiledRules(ctx context.Context, userID int32) (map[api.RepoName]compiledRules, error) {
	// Fast path for cached rules
	item, _ := s.cache.Get(userID)
	cached, ok := item.(cachedRules)

	ttl := defaultCacheTTL
	if c := conf.Get(); c.ExperimentalFeatures != nil && c.ExperimentalFeatures.SubRepoPermissions != nil && c.ExperimentalFeatures.SubRepoPermissions.UserCacheTTLSeconds > 0 {
		ttl = time.Duration(c.ExperimentalFeatures.SubRepoPermissions.UserCacheTTLSeconds) * time.Second
	}

	if ok && s.since(cached.timestamp) <= ttl {
		subRepoPermsCacheHit.WithLabelValues("true").Inc()
		return cached.rules, nil
	}
	subRepoPermsCacheHit.WithLabelValues("false").Inc()

	// Slow path on cache miss or expiry. Ensure that only one goroutine is doing the
	// work
	groupKey := strconv.FormatInt(int64(userID), 10)
	result, err, _ := s.group.Do(groupKey, func() (any, error) {
		repoPerms, err := s.permissionsGetter.GetByUser(ctx, userID)
		if err != nil {
			return nil, errors.Wrap(err, "fetching rules")
		}
		toCache := cachedRules{
			rules:     make(map[api.RepoName]compiledRules, len(repoPerms)),
			timestamp: time.Time{},
		}
		for repo, perms := range repoPerms {
			includes := make([]glob.Glob, 0, len(perms.PathIncludes))
			dirIncludes := make([]glob.Glob, 0)
			dirSeen := make(map[string]struct{})
			for _, rule := range perms.PathIncludes {
				g, err := glob.Compile(rule, '/')
				if err != nil {
					return nil, errors.Wrap(err, "building include matcher")
				}
				includes = append(includes, g)

				// We should include all directories above an include rule
				dirs := expandDirs(rule)
				for _, dir := range dirs {
					if _, ok := dirSeen[dir]; ok {
						continue
					}
					g, err := glob.Compile(dir, '/')
					if err != nil {
						return nil, errors.Wrap(err, "building include matcher for dir")
					}
					dirIncludes = append(dirIncludes, g)
					dirSeen[dir] = struct{}{}
				}
			}

			excludes := make([]glob.Glob, 0, len(perms.PathExcludes))
			for _, rule := range perms.PathExcludes {
				g, err := glob.Compile(rule, '/')
				if err != nil {
					return nil, errors.Wrap(err, "building exclude matcher")
				}
				excludes = append(excludes, g)
			}
			toCache.rules[repo] = compiledRules{
				includes:    includes,
				excludes:    excludes,
				dirIncludes: dirIncludes,
			}
		}
		toCache.timestamp = s.clock()
		s.cache.Add(userID, toCache)
		return toCache.rules, nil
	})
	if err != nil {
		return nil, err
	}

	compiled := result.(map[api.RepoName]compiledRules)
	return compiled, nil
}

func (s *SubRepoPermsClient) Enabled() bool {
	if c := conf.Get(); c.ExperimentalFeatures != nil && c.ExperimentalFeatures.SubRepoPermissions != nil {
		return c.ExperimentalFeatures.SubRepoPermissions.Enabled
	}
	return false
}

func (s *SubRepoPermsClient) EnabledForRepoId(ctx context.Context, id api.RepoID) (bool, error) {
	return s.permissionsGetter.RepoIdSupported(ctx, id)
}

func (s *SubRepoPermsClient) EnabledForRepo(ctx context.Context, repo api.RepoName) (bool, error) {
	return s.permissionsGetter.RepoSupported(ctx, repo)
}

// expandDirs will return rules that match all parent directories of the given
// rule.
func expandDirs(rule string) []string {
	dirs := make([]string, 0)

	// We can't support rules that start with a wildcard because we can only
	// see one level of the tree at a time so we have no way of knowing which path leads
	// to a file the user is allowed to see.
	if strings.HasPrefix(rule, "*") {
		return dirs
	}

	for {
		lastSlash := strings.LastIndex(rule, "/")
		if lastSlash == -1 {
			break
		}
		// Drop anything after the last slash
		rule = rule[:lastSlash]

		dirs = append(dirs, rule+"/")
	}

	return dirs
}

// NewSimpleChecker is exposed for testing and allows creation of a simple
// checker based on the rules provided. The rules are expected to be in glob
// format.
func NewSimpleChecker(repo api.RepoName, includes []string, excludes []string) (SubRepoPermissionChecker, error) {
	getter := NewMockSubRepoPermissionsGetter()
	getter.GetByUserFunc.SetDefaultHook(func(ctx context.Context, i int32) (map[api.RepoName]SubRepoPermissions, error) {
		return map[api.RepoName]SubRepoPermissions{
			repo: {
				PathIncludes: includes,
				PathExcludes: excludes,
			},
		}, nil
	})
	getter.RepoSupportedFunc.SetDefaultReturn(true, nil)
	getter.RepoIdSupportedFunc.SetDefaultReturn(true, nil)
	return NewSubRepoPermsClient(getter)
}

// ActorPermissions returns the level of access the given actor has for the requested
// content.
//
// If the context is unauthenticated, ErrUnauthenticated is returned. If the context is
// internal, Read permissions is granted.
func ActorPermissions(ctx context.Context, s SubRepoPermissionChecker, a *actor.Actor, content RepoContent) (Perms, error) {
	// Check config here, despite checking again in the s.Permissions implementation,
	// because we also make some permissions decisions here.
	if !SubRepoEnabled(s) {
		return Read, nil
	}
	if a.IsInternal() {
		return Read, nil
	}
	if !a.IsAuthenticated() {
		return None, &ErrUnauthenticated{}
	}

	perms, err := s.Permissions(ctx, a.UID, content)
	if err != nil {
		return None, errors.Wrapf(err, "getting actor permissions for actor: %d", a.UID)
	}
	return perms, nil
}

// SubRepoEnabled takes a SubRepoPermissionChecker and returns true if the checker is not nil and is enabled
func SubRepoEnabled(checker SubRepoPermissionChecker) bool {
	return checker != nil && checker.Enabled()
}

// SubRepoEnabledForRepoID takes a SubRepoPermissionChecker and repoID and returns true if sub-repo
// permissions are enabled for a repo with given repoID
func SubRepoEnabledForRepoID(ctx context.Context, checker SubRepoPermissionChecker, repoID api.RepoID) (bool, error) {
	if !SubRepoEnabled(checker) {
		return false, nil
	}
	return checker.EnabledForRepoId(ctx, repoID)
}

// SubRepoEnabledForRepo takes a SubRepoPermissionChecker and repo name and returns true if sub-repo
// permissions are enabled for the given repo
func SubRepoEnabledForRepo(ctx context.Context, checker SubRepoPermissionChecker, repo api.RepoName) (bool, error) {
	if !SubRepoEnabled(checker) {
		return false, nil
	}
	return checker.EnabledForRepo(ctx, repo)
}

// CanReadAllPaths returns true if the actor can read all paths.
func CanReadAllPaths(ctx context.Context, checker SubRepoPermissionChecker, repo api.RepoName, paths []string) (bool, error) {
	if !SubRepoEnabled(checker) {
		return true, nil
	}
	a := actor.FromContext(ctx)
	if a.IsInternal() {
		return true, nil
	}
	if !a.IsAuthenticated() {
		return false, &ErrUnauthenticated{}
	}

	c := RepoContent{
		Repo: repo,
	}

	for _, p := range paths {
		c.Path = p
		perms, err := checker.Permissions(ctx, a.UID, c)
		if err != nil {
			return false, err
		}
		if !perms.Include(Read) {
			return false, nil
		}
	}

	return true, nil
}

// FilterActorPaths will filter the given list of paths for the given actor
// returning on paths they are allowed to read.
func FilterActorPaths(ctx context.Context, checker SubRepoPermissionChecker, a *actor.Actor, repo api.RepoName, paths []string) ([]string, error) {
	filtered := make([]string, 0, len(paths))
	for _, p := range paths {
		include, err := FilterActorPath(ctx, checker, a, repo, p)
		if err != nil {
			return nil, errors.Wrap(err, "checking sub-repo permissions")
		}
		if include {
			filtered = append(filtered, p)
		}
	}
	return filtered, nil
}

// FilterActorPath will filter the given path for the given actor
// returning true if the path is allowed to read.
func FilterActorPath(ctx context.Context, checker SubRepoPermissionChecker, a *actor.Actor, repo api.RepoName, path string) (bool, error) {
	if !SubRepoEnabled(checker) {
		return true, nil
	}
	perms, err := ActorPermissions(ctx, checker, a, RepoContent{
		Repo: repo,
		Path: path,
	})
	if err != nil {
		return false, errors.Wrap(err, "checking sub-repo permissions")
	}
	return perms.Include(Read), nil
}

func FilterActorFileInfos(ctx context.Context, checker SubRepoPermissionChecker, a *actor.Actor, repo api.RepoName, fis []fs.FileInfo) ([]fs.FileInfo, error) {
	filtered := make([]fs.FileInfo, 0, len(fis))
	for _, fi := range fis {
		include, err := FilterActorFileInfo(ctx, checker, a, repo, fi)
		if err != nil {
			return nil, err
		}
		if include {
			filtered = append(filtered, fi)
		}
	}
	return filtered, nil
}

func FilterActorFileInfo(ctx context.Context, checker SubRepoPermissionChecker, a *actor.Actor, repo api.RepoName, fi fs.FileInfo) (bool, error) {
	rc := repoContentFromFileInfo(repo, fi)
	perms, err := ActorPermissions(ctx, checker, a, rc)
	if err != nil {
		return false, errors.Wrap(err, "checking sub-repo permissions")
	}
	return perms.Include(Read), nil
}

func repoContentFromFileInfo(repo api.RepoName, fi fs.FileInfo) RepoContent {
	rc := RepoContent{
		Repo: repo,
		Path: fi.Name(),
	}
	if fi.IsDir() {
		rc.Path += "/"
	}
	return rc
}
