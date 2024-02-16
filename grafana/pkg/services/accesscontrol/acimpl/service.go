package acimpl

import (
	"context"
	"fmt"
	"slices"
	"strconv"
	"strings"
	"time"

	"github.com/prometheus/client_golang/prometheus"

	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/localcache"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/metrics"
	"github.com/grafana/grafana/pkg/infra/slugify"
	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/accesscontrol/api"
	"github.com/grafana/grafana/pkg/services/accesscontrol/database"
	"github.com/grafana/grafana/pkg/services/accesscontrol/migrator"
	"github.com/grafana/grafana/pkg/services/accesscontrol/pluginutils"
	"github.com/grafana/grafana/pkg/services/auth/identity"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
)

var _ plugins.RoleRegistry = &Service{}

const (
	cacheTTL = 10 * time.Second
)

var SharedWithMeFolderPermission = accesscontrol.Permission{
	Action: dashboards.ActionFoldersRead,
	Scope:  dashboards.ScopeFoldersProvider.GetResourceScopeUID(folder.SharedWithMeFolderUID),
}

func ProvideService(cfg *setting.Cfg, db db.DB, routeRegister routing.RouteRegister, cache *localcache.CacheService,
	accessControl accesscontrol.AccessControl, features featuremgmt.FeatureToggles) (*Service, error) {
	service := ProvideOSSService(cfg, database.ProvideService(db), cache, features)

	api.NewAccessControlAPI(routeRegister, accessControl, service, features).RegisterAPIEndpoints()
	if err := accesscontrol.DeclareFixedRoles(service, cfg); err != nil {
		return nil, err
	}

	// Migrating scopes that haven't been split yet to have kind, attribute and identifier in the DB
	// This will be removed once we've:
	// 1) removed the feature toggle and
	// 2) have released enough versions not to support a version without split scopes
	if err := migrator.MigrateScopeSplit(db, service.log); err != nil {
		return nil, err
	}

	return service, nil
}

func ProvideOSSService(cfg *setting.Cfg, store store, cache *localcache.CacheService, features featuremgmt.FeatureToggles) *Service {
	s := &Service{
		cache:    cache,
		cfg:      cfg,
		features: features,
		log:      log.New("accesscontrol.service"),
		roles:    accesscontrol.BuildBasicRoleDefinitions(),
		store:    store,
	}

	return s
}

//go:generate  mockery --name store --structname MockStore --outpkg actest --filename store_mock.go --output ../actest/
type store interface {
	GetUserPermissions(ctx context.Context, query accesscontrol.GetUserPermissionsQuery) ([]accesscontrol.Permission, error)
	SearchUsersPermissions(ctx context.Context, orgID int64, options accesscontrol.SearchOptions) (map[int64][]accesscontrol.Permission, error)
	GetUsersBasicRoles(ctx context.Context, userFilter []int64, orgID int64) (map[int64][]string, error)
	DeleteUserPermissions(ctx context.Context, orgID, userID int64) error
	SaveExternalServiceRole(ctx context.Context, cmd accesscontrol.SaveExternalServiceRoleCommand) error
	DeleteExternalServiceRole(ctx context.Context, externalServiceID string) error
}

// Service is the service implementing role based access control.
type Service struct {
	cache         *localcache.CacheService
	cfg           *setting.Cfg
	features      featuremgmt.FeatureToggles
	log           log.Logger
	registrations accesscontrol.RegistrationList
	roles         map[string]*accesscontrol.RoleDTO
	store         store
}

func (s *Service) GetUsageStats(_ context.Context) map[string]any {
	return map[string]any{
		"stats.oss.accesscontrol.enabled.count": 1,
	}
}

// GetUserPermissions returns user permissions based on built-in roles
func (s *Service) GetUserPermissions(ctx context.Context, user identity.Requester, options accesscontrol.Options) ([]accesscontrol.Permission, error) {
	timer := prometheus.NewTimer(metrics.MAccessPermissionsSummary)
	defer timer.ObserveDuration()

	if !s.cfg.RBACPermissionCache || !user.HasUniqueId() {
		return s.getUserPermissions(ctx, user, options)
	}

	return s.getCachedUserPermissions(ctx, user, options)
}

func (s *Service) getUserPermissions(ctx context.Context, user identity.Requester, options accesscontrol.Options) ([]accesscontrol.Permission, error) {
	permissions := make([]accesscontrol.Permission, 0)
	for _, builtin := range accesscontrol.GetOrgRoles(user) {
		if basicRole, ok := s.roles[builtin]; ok {
			permissions = append(permissions, basicRole.Permissions...)
		}
	}

	if s.features.IsEnabled(ctx, featuremgmt.FlagNestedFolders) {
		permissions = append(permissions, SharedWithMeFolderPermission)
	}

	userID, err := identity.UserIdentifier(user.GetNamespacedID())
	if err != nil {
		return nil, err
	}

	dbPermissions, err := s.store.GetUserPermissions(ctx, accesscontrol.GetUserPermissionsQuery{
		OrgID:        user.GetOrgID(),
		UserID:       userID,
		Roles:        accesscontrol.GetOrgRoles(user),
		TeamIDs:      user.GetTeams(),
		RolePrefixes: []string{accesscontrol.ManagedRolePrefix, accesscontrol.ExternalServiceRolePrefix},
	})
	if err != nil {
		return nil, err
	}

	return append(permissions, dbPermissions...), nil
}

func (s *Service) getCachedUserPermissions(ctx context.Context, user identity.Requester, options accesscontrol.Options) ([]accesscontrol.Permission, error) {
	key := permissionCacheKey(user)
	if !options.ReloadCache {
		permissions, ok := s.cache.Get(key)
		if ok {
			metrics.MAccessPermissionsCacheUsage.WithLabelValues(accesscontrol.CacheHit).Inc()
			s.log.Debug("Using cached permissions", "key", key)
			return permissions.([]accesscontrol.Permission), nil
		}
	}

	metrics.MAccessPermissionsCacheUsage.WithLabelValues(accesscontrol.CacheMiss).Inc()
	s.log.Debug("Fetch permissions from store", "key", key)
	permissions, err := s.getUserPermissions(ctx, user, options)
	if err != nil {
		return nil, err
	}

	s.log.Debug("Cache permissions", "key", key)
	s.cache.Set(key, permissions, cacheTTL)

	return permissions, nil
}

func (s *Service) ClearUserPermissionCache(user identity.Requester) {
	s.cache.Delete(permissionCacheKey(user))
}

func (s *Service) DeleteUserPermissions(ctx context.Context, orgID int64, userID int64) error {
	return s.store.DeleteUserPermissions(ctx, orgID, userID)
}

// DeclareFixedRoles allow the caller to declare, to the service, fixed roles and their assignments
// to organization roles ("Viewer", "Editor", "Admin") or "Grafana Admin"
func (s *Service) DeclareFixedRoles(registrations ...accesscontrol.RoleRegistration) error {
	for _, r := range registrations {
		err := accesscontrol.ValidateFixedRole(r.Role)
		if err != nil {
			return err
		}

		err = accesscontrol.ValidateBuiltInRoles(r.Grants)
		if err != nil {
			return err
		}

		s.registrations.Append(r)
	}

	return nil
}

// RegisterFixedRoles registers all declared roles in RAM
func (s *Service) RegisterFixedRoles(ctx context.Context) error {
	s.registrations.Range(func(registration accesscontrol.RoleRegistration) bool {
		for br := range accesscontrol.BuiltInRolesWithParents(registration.Grants) {
			if basicRole, ok := s.roles[br]; ok {
				basicRole.Permissions = append(basicRole.Permissions, registration.Role.Permissions...)
			} else {
				s.log.Error("Unknown builtin role", "builtInRole", br)
			}
		}
		return true
	})
	return nil
}

func permissionCacheKey(user identity.Requester) string {
	return fmt.Sprintf("rbac-permissions-%s", user.GetCacheKey())
}

// DeclarePluginRoles allow the caller to declare, to the service, plugin roles and their assignments
// to organization roles ("Viewer", "Editor", "Admin") or "Grafana Admin"
func (s *Service) DeclarePluginRoles(ctx context.Context, ID, name string, regs []plugins.RoleRegistration) error {
	// Protect behind feature toggle
	if !s.features.IsEnabled(ctx, featuremgmt.FlagAccessControlOnCall) {
		return nil
	}

	acRegs := pluginutils.ToRegistrations(ID, name, regs)
	for _, r := range acRegs {
		if err := pluginutils.ValidatePluginRole(ID, r.Role); err != nil {
			return err
		}

		if err := accesscontrol.ValidateBuiltInRoles(r.Grants); err != nil {
			return err
		}

		s.log.Debug("Registering plugin role", "role", r.Role.Name)
		s.registrations.Append(r)
	}

	return nil
}

// SearchUsersPermissions returns all users' permissions filtered by action prefixes
func (s *Service) SearchUsersPermissions(ctx context.Context, usr identity.Requester,
	options accesscontrol.SearchOptions) (map[int64][]accesscontrol.Permission, error) {
	if options.NamespacedID != "" {
		userID, err := options.ComputeUserID()
		if err != nil {
			s.log.Error("Failed to resolve user ID", "error", err)
			return nil, err
		}

		// Reroute to the user specific implementation of search permissions
		// because it leverages the user permission cache.
		userPerms, err := s.SearchUserPermissions(ctx, usr.GetOrgID(), options)
		if err != nil {
			return nil, err
		}
		return map[int64][]accesscontrol.Permission{userID: userPerms}, nil
	}

	timer := prometheus.NewTimer(metrics.MAccessSearchPermissionsSummary)
	defer timer.ObserveDuration()

	// Filter ram permissions
	basicPermissions := map[string][]accesscontrol.Permission{}
	for role, basicRole := range s.roles {
		for i := range basicRole.Permissions {
			if PermissionMatchesSearchOptions(basicRole.Permissions[i], &options) {
				basicPermissions[role] = append(basicPermissions[role], basicRole.Permissions[i])
			}
		}
	}

	usersRoles, err := s.store.GetUsersBasicRoles(ctx, nil, usr.GetOrgID())
	if err != nil {
		return nil, err
	}

	// Get managed permissions (DB)
	usersPermissions, err := s.store.SearchUsersPermissions(ctx, usr.GetOrgID(), options)
	if err != nil {
		return nil, err
	}

	// helper to filter out permissions the signed in users cannot see
	canView := func() func(userID int64) bool {
		siuPermissions := usr.GetPermissions()
		if len(siuPermissions) == 0 {
			return func(_ int64) bool { return false }
		}
		scopes, ok := siuPermissions[accesscontrol.ActionUsersPermissionsRead]
		if !ok {
			return func(_ int64) bool { return false }
		}

		ids := map[int64]bool{}
		for i := range scopes {
			if strings.HasSuffix(scopes[i], "*") {
				return func(_ int64) bool { return true }
			}
			parts := strings.Split(scopes[i], ":")
			if len(parts) != 3 {
				continue
			}
			id, err := strconv.ParseInt(parts[2], 10, 64)
			if err != nil {
				continue
			}
			ids[id] = true
		}

		return func(userID int64) bool { return ids[userID] }
	}()

	// Merge stored (DB) and basic role permissions (RAM)
	// Assumes that all users with stored permissions have org roles
	res := map[int64][]accesscontrol.Permission{}
	for userID, roles := range usersRoles {
		if !canView(userID) {
			continue
		}
		perms := []accesscontrol.Permission{}
		for i := range roles {
			basicPermission, ok := basicPermissions[roles[i]]
			if !ok {
				continue
			}
			perms = append(perms, basicPermission...)
		}
		if dbPerms, ok := usersPermissions[userID]; ok {
			perms = append(perms, dbPerms...)
		}
		if len(perms) > 0 {
			res[userID] = perms
		}
	}

	return res, nil
}

func (s *Service) SearchUserPermissions(ctx context.Context, orgID int64, searchOptions accesscontrol.SearchOptions) ([]accesscontrol.Permission, error) {
	timer := prometheus.NewTimer(metrics.MAccessPermissionsSummary)
	defer timer.ObserveDuration()

	if searchOptions.NamespacedID == "" {
		return nil, fmt.Errorf("expected namespaced ID to be specified")
	}

	if permissions, success := s.searchUserPermissionsFromCache(orgID, searchOptions); success {
		return permissions, nil
	}
	return s.searchUserPermissions(ctx, orgID, searchOptions)
}

func (s *Service) searchUserPermissions(ctx context.Context, orgID int64, searchOptions accesscontrol.SearchOptions) ([]accesscontrol.Permission, error) {
	userID, err := searchOptions.ComputeUserID()
	if err != nil {
		return nil, err
	}

	// Get permissions for user's basic roles from RAM
	roleList, err := s.store.GetUsersBasicRoles(ctx, []int64{userID}, orgID)
	if err != nil {
		return nil, fmt.Errorf("could not fetch basic roles for the user: %w", err)
	}
	var roles []string
	var ok bool
	if roles, ok = roleList[userID]; !ok {
		return nil, fmt.Errorf("found no basic roles for user %d in organisation %d", userID, orgID)
	}
	permissions := make([]accesscontrol.Permission, 0)
	for _, builtin := range roles {
		if basicRole, ok := s.roles[builtin]; ok {
			for _, permission := range basicRole.Permissions {
				if PermissionMatchesSearchOptions(permission, &searchOptions) {
					permissions = append(permissions, permission)
				}
			}
		}
	}

	// Get permissions from the DB
	dbPermissions, err := s.store.SearchUsersPermissions(ctx, orgID, searchOptions)
	if err != nil {
		return nil, err
	}
	permissions = append(permissions, dbPermissions[userID]...)

	return permissions, nil
}

func (s *Service) searchUserPermissionsFromCache(orgID int64, searchOptions accesscontrol.SearchOptions) ([]accesscontrol.Permission, bool) {
	userID, err := searchOptions.ComputeUserID()
	if err != nil {
		return nil, false
	}

	// Create a temp signed in user object to retrieve cache key
	tempUser := &user.SignedInUser{
		UserID: userID,
		OrgID:  orgID,
	}

	key := permissionCacheKey(tempUser)
	permissions, ok := s.cache.Get((key))
	if !ok {
		metrics.MAccessSearchUserPermissionsCacheUsage.WithLabelValues(accesscontrol.CacheMiss).Inc()
		return nil, false
	}

	metrics.MAccessSearchUserPermissionsCacheUsage.WithLabelValues(accesscontrol.CacheHit).Inc()

	s.log.Debug("Using cached permissions", "key", key)
	filteredPermissions := make([]accesscontrol.Permission, 0)
	for _, permission := range permissions.([]accesscontrol.Permission) {
		if PermissionMatchesSearchOptions(permission, &searchOptions) {
			filteredPermissions = append(filteredPermissions, permission)
		}
	}

	return filteredPermissions, true
}

func PermissionMatchesSearchOptions(permission accesscontrol.Permission, searchOptions *accesscontrol.SearchOptions) bool {
	if searchOptions.Scope != "" {
		// Permissions including the scope should also match
		scopes := append(searchOptions.Wildcards(), searchOptions.Scope)
		if !slices.Contains[[]string, string](scopes, permission.Scope) {
			return false
		}
	}
	if searchOptions.Action != "" {
		return permission.Action == searchOptions.Action
	}
	return strings.HasPrefix(permission.Action, searchOptions.ActionPrefix)
}

func (s *Service) SaveExternalServiceRole(ctx context.Context, cmd accesscontrol.SaveExternalServiceRoleCommand) error {
	if !(s.features.IsEnabled(ctx, featuremgmt.FlagExternalServiceAuth) || s.features.IsEnabled(ctx, featuremgmt.FlagExternalServiceAccounts)) {
		s.log.Debug("Registering an external service role is behind a feature flag, enable it to use this feature.")
		return nil
	}

	if err := cmd.Validate(); err != nil {
		return err
	}

	return s.store.SaveExternalServiceRole(ctx, cmd)
}

func (s *Service) DeleteExternalServiceRole(ctx context.Context, externalServiceID string) error {
	if !(s.features.IsEnabled(ctx, featuremgmt.FlagExternalServiceAuth) || s.features.IsEnabled(ctx, featuremgmt.FlagExternalServiceAccounts)) {
		s.log.Debug("Deleting an external service role is behind a feature flag, enable it to use this feature.")
		return nil
	}

	slug := slugify.Slugify(externalServiceID)

	return s.store.DeleteExternalServiceRole(ctx, slug)
}

func (*Service) SyncUserRoles(ctx context.Context, orgID int64, cmd accesscontrol.SyncUserRolesCommand) error {
	return nil
}
