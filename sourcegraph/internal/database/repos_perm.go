package database

import (
	"context"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var errPermissionsUserMappingConflict = errors.New("The permissions user mapping (site configuration `permissions.userMapping`) cannot be enabled when other authorization providers are in use, please contact site admin to resolve it.")

type BypassAuthzReason = string

const (
	BypassAuthzReasonSiteAdmin       BypassAuthzReason = "Site Admin"
	BypassAuthzReasonIsInternal      BypassAuthzReason = "Internal Request"
	BypassAuthzReasonNoAuthzProvider BypassAuthzReason = "No Authz Provider Configured"
)

type AuthzQueryParameters struct {
	BypassAuthz               bool
	BypassAuthzReason         BypassAuthzReason
	UsePermissionsUserMapping bool
	AuthenticatedUserID       int32
}

func (p *AuthzQueryParameters) ToAuthzQuery() *sqlf.Query {
	return authzQuery(
		p.BypassAuthz,
		p.UsePermissionsUserMapping,
		p.AuthenticatedUserID,
	)
}

func GetAuthzQueryParameters(ctx context.Context, db DB) (params *AuthzQueryParameters, err error) {
	params = &AuthzQueryParameters{}
	authzAllowByDefault, authzProviders := authz.GetProviders()
	params.UsePermissionsUserMapping = globals.PermissionsUserMapping().Enabled

	// 🚨 SECURITY: Blocking access to all repositories if both code host authz
	// provider(s) and permissions user mapping are configured.
	if params.UsePermissionsUserMapping {
		if len(authzProviders) > 0 {
			return nil, errPermissionsUserMappingConflict
		}
		authzAllowByDefault = false
	}

	a := actor.FromContext(ctx)

	// Authz is bypassed when the request is coming from an internal actor or
	// there is no authz provider configured and access to all repositories are
	// allowed by default. Authz can be bypassed by site admins unless
	// conf.AuthEnforceForSiteAdmins is set to "true".
	//
	// 🚨 SECURITY: internal requests bypass authz provider permissions checks,
	// so correctness is important here.
	if a.IsInternal() {
		params.BypassAuthz = true
		params.BypassAuthzReason = BypassAuthzReasonIsInternal
	} else if authzAllowByDefault && len(authzProviders) == 0 {
		params.BypassAuthz = true
		params.BypassAuthzReason = BypassAuthzReasonNoAuthzProvider
	} else if a.IsAuthenticated() {
		currentUser, err := db.Users().GetByCurrentAuthUser(ctx)
		if err != nil {
			return nil, err
		}
		params.AuthenticatedUserID = currentUser.ID
		params.BypassAuthz = currentUser.SiteAdmin && !conf.Get().AuthzEnforceForSiteAdmins

		if params.BypassAuthz {
			params.BypassAuthzReason = BypassAuthzReasonSiteAdmin
		}
	}

	return params, err
}

// AuthzQueryConds returns a query clause for enforcing repository permissions.
// It uses `repo` as the table name to filter out repository IDs and should be
// used as an AND condition in a complete SQL query.
func AuthzQueryConds(ctx context.Context, db DB) (*sqlf.Query, error) {
	params, err := GetAuthzQueryParameters(ctx, db)
	if err != nil {
		return nil, err
	}

	return params.ToAuthzQuery(), nil
}

//nolint:unparam // unparam complains that `perms` always has same value across call-sites, but that's OK, as we only support read permissions right now.
func authzQuery(bypassAuthz, usePermissionsUserMapping bool, authenticatedUserID int32) *sqlf.Query {
	if bypassAuthz {
		// if bypassAuthz is true, we don't care about any of the checks
		return sqlf.Sprintf(`
(
    -- Bypass authz
    TRUE
)
`)
	}

	unifiedPermsEnabled := conf.ExperimentalFeatures().UnifiedPermissions

	unrestrictedReposSQL := `
	-- Unrestricted repos are visible to all users
	EXISTS (
		SELECT
		FROM user_repo_permissions
		WHERE repo_id = repo.id AND user_id IS NULL
	)
	`
	if !unifiedPermsEnabled {
		unrestrictedReposSQL = `
	-- Unrestricted repos are visible to all users
	EXISTS (
		SELECT
		FROM repo_permissions
		WHERE repo_id = repo.id
		AND unrestricted
	)
		`
	}

	conditions := []*sqlf.Query{sqlf.Sprintf(unrestrictedReposSQL)}

	// Disregard unrestricted state when permissions user mapping is enabled
	if !usePermissionsUserMapping {
		const externalServiceUnrestrictedSQL = `
(
    NOT repo.private          -- Happy path of non-private repositories
    OR  EXISTS (              -- Each external service defines if repositories are unrestricted
        SELECT
        FROM external_services AS es
        JOIN external_service_repos AS esr ON (
                esr.external_service_id = es.id
            AND esr.repo_id = repo.id
            AND es.unrestricted = TRUE
            AND es.deleted_at IS NULL
        )
	)
)
`
		externalServiceUnrestrictedQuery := sqlf.Sprintf(externalServiceUnrestrictedSQL)
		conditions = append(conditions, externalServiceUnrestrictedQuery)
	}

	restrictedRepositoriesSQL := `
	-- Restricted repositories require checking permissions
	EXISTS (
		SELECT repo_id FROM user_repo_permissions
		WHERE
			repo_id = repo.id
		AND user_id = %s
	)
	`
	if !unifiedPermsEnabled {
		restrictedRepositoriesSQL = `
	-- Restricted repositories require checking permissions
    (
		SELECT object_ids_ints @> INTSET(repo.id)
		FROM user_permissions
		WHERE
			user_id = %s
		AND permission = 'read'
		AND object_type = 'repos'
	)
	`
	}
	restrictedRepositoriesQuery := sqlf.Sprintf(restrictedRepositoriesSQL, authenticatedUserID)

	conditions = append(conditions, restrictedRepositoriesQuery)

	// Have to manually wrap the result in parenthesis so that they're evaluated together
	return sqlf.Sprintf("(%s)", sqlf.Join(conditions, "\nOR\n"))
}
