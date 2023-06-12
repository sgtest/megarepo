package authz

import (
	"context"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/enterprise"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/hooks"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/authz/resolvers"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/authz/webhooks"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/licensing/enforcement"
	eiauthz "github.com/sourcegraph/sourcegraph/enterprise/internal/authz"
	srp "github.com/sourcegraph/sourcegraph/enterprise/internal/authz/subrepoperms"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var clock = timeutil.Now

func Init(
	ctx context.Context,
	observationCtx *observation.Context,
	db database.DB,
	_ codeintel.Services,
	_ conftypes.UnifiedWatchable,
	enterpriseServices *enterprise.Services,
) error {
	database.ValidateExternalServiceConfig = edb.ValidateExternalServiceConfig
	database.AuthzWith = func(other basestore.ShareableStore) database.AuthzStore {
		return edb.NewAuthzStore(observationCtx.Logger, db, clock)
	}

	extsvcStore := db.ExternalServices()

	// TODO(nsc): use c
	// Report any authz provider problems in external configs.
	conf.ContributeWarning(func(cfg conftypes.SiteConfigQuerier) (problems conf.Problems) {
		_, providers, seriousProblems, warnings, _ := eiauthz.ProvidersFromConfig(ctx, cfg, extsvcStore, db)
		problems = append(problems, conf.NewExternalServiceProblems(seriousProblems...)...)

		// Validating the connection may make a cross service call, so we should use an
		// internal actor.
		ctx := actor.WithInternalActor(ctx)

		// Add connection validation issue
		for _, p := range providers {
			if err := p.ValidateConnection(ctx); err != nil {
				warnings = append(warnings, fmt.Sprintf("%s provider %q: %s", p.ServiceType(), p.ServiceID(), err))
			}
		}

		problems = append(problems, conf.NewExternalServiceProblems(warnings...)...)
		return problems
	})

	enterpriseServices.PermissionsGitHubWebhook = webhooks.NewGitHubWebhook(log.Scoped("PermissionsGitHubWebhook", "permissions sync webhook handler for GitHub webhooks"))

	var err error
	authz.DefaultSubRepoPermsChecker, err = srp.NewSubRepoPermsClient(edb.NewEnterpriseDB(db).SubRepoPerms())
	if err != nil {
		return errors.Wrap(err, "Failed to createe sub-repo client")
	}

	graphqlbackend.AlertFuncs = append(graphqlbackend.AlertFuncs, func(args graphqlbackend.AlertFuncArgs) []*graphqlbackend.Alert {
		if licensing.IsLicenseValid() {
			return nil
		}

		return []*graphqlbackend.Alert{{
			TypeValue:    graphqlbackend.AlertTypeError,
			MessageValue: "To continue using Sourcegraph, a site admin must renew the Sourcegraph license (or downgrade to only using Sourcegraph Free features). Update the license key in the [**site configuration**](/site-admin/configuration).",
		}}
	})

	// Warn about usage of authz providers that are not enabled by the license.
	graphqlbackend.AlertFuncs = append(graphqlbackend.AlertFuncs, func(args graphqlbackend.AlertFuncArgs) []*graphqlbackend.Alert {
		// Only site admins can act on this alert, so only show it to site admins.
		if !args.IsSiteAdmin {
			return nil
		}

		if licensing.IsFeatureEnabledLenient(licensing.FeatureACLs) {
			return nil
		}

		_, _, _, _, invalidConnections := eiauthz.ProvidersFromConfig(ctx, conf.Get(), extsvcStore, db)

		// We currently support three types of authz providers: GitHub, GitLab and Bitbucket Server.
		authzTypes := make(map[string]struct{}, 3)
		for _, conn := range invalidConnections {
			authzTypes[conn] = struct{}{}
		}

		authzNames := make([]string, 0, len(authzTypes))
		for t := range authzTypes {
			switch t {
			case extsvc.TypeGitHub:
				authzNames = append(authzNames, "GitHub")
			case extsvc.TypeGitLab:
				authzNames = append(authzNames, "GitLab")
			case extsvc.TypeBitbucketServer:
				authzNames = append(authzNames, "Bitbucket Server")
			default:
				authzNames = append(authzNames, t)
			}
		}

		if len(authzNames) == 0 {
			return nil
		}

		return []*graphqlbackend.Alert{{
			TypeValue:    graphqlbackend.AlertTypeError,
			MessageValue: fmt.Sprintf("A Sourcegraph license is required to enable repository permissions for the following code hosts: %s. [**Get a license.**](/site-admin/license)", strings.Join(authzNames, ", ")),
		}}
	})

	graphqlbackend.AlertFuncs = append(graphqlbackend.AlertFuncs, func(args graphqlbackend.AlertFuncArgs) []*graphqlbackend.Alert {
		// 🚨 SECURITY: Only the site admin should ever see this (all other users will see a hard-block
		// license expiration screen) about this. Leaking this wouldn't be a security vulnerability, but
		// just in case this method is changed to return more information, we lock it down.
		if !args.IsSiteAdmin {
			return nil
		}

		info, err := licensing.GetConfiguredProductLicenseInfo()
		if err != nil {
			observationCtx.Logger.Error("Error reading license key for Sourcegraph subscription.", log.Error(err))
			return []*graphqlbackend.Alert{{
				TypeValue:    graphqlbackend.AlertTypeError,
				MessageValue: "Error reading Sourcegraph license key. Check the logs for more information, or update the license key in the [**site configuration**](/site-admin/configuration).",
			}}
		}
		if info != nil && info.IsExpired() {
			return []*graphqlbackend.Alert{{
				TypeValue:    graphqlbackend.AlertTypeError,
				MessageValue: "Sourcegraph license expired! All non-admin users are locked out of Sourcegraph. Update the license key in the [**site configuration**](/site-admin/configuration) or downgrade to only using Sourcegraph Free features.",
			}}
		}
		if info != nil && info.IsExpiringSoon() {
			return []*graphqlbackend.Alert{{
				TypeValue:    graphqlbackend.AlertTypeWarning,
				MessageValue: fmt.Sprintf("Sourcegraph license will expire soon! Expires on: %s. Update the license key in the [**site configuration**](/site-admin/configuration) or downgrade to only using Sourcegraph Free features.", info.ExpiresAt.UTC().Truncate(time.Hour).Format(time.UnixDate)),
			}}
		}
		return nil
	})

	// Enforce the use of a valid license key by preventing all HTTP requests if the license is invalid
	// (due to an error in parsing or verification, or because the license has expired).
	hooks.PostAuthMiddleware = func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			a := actor.FromContext(ctx)
			// Ignore not authenticated users, because we need to allow site admins
			// to sign in to set a license.
			if !a.IsAuthenticated() {
				next.ServeHTTP(w, r)
				return
			}

			siteadminOrHandler := func(handler func()) {
				err := auth.CheckCurrentUserIsSiteAdmin(r.Context(), db)
				if err == nil {
					// User is site admin, let them proceed.
					next.ServeHTTP(w, r)
					return
				}
				if err != auth.ErrMustBeSiteAdmin {
					observationCtx.Logger.Error("Error checking current user is site admin", log.Error(err))
					http.Error(w, "Error checking current user is site admin. Site admins may check the logs for more information.", http.StatusInternalServerError)
					return
				}

				handler()
			}

			// Check if there are any license issues. If so, don't let the request go through.
			// Exception: Site admins are exempt from license enforcement screens so that they
			// can easily update the license key. We only fetch the user if we don't have a license,
			// to save that DB lookup in most cases.
			info, err := licensing.GetConfiguredProductLicenseInfo()
			if err != nil {
				observationCtx.Logger.Error("Error reading license key for Sourcegraph subscription.", log.Error(err))
				siteadminOrHandler(func() {
					enforcement.WriteSubscriptionErrorResponse(w, http.StatusInternalServerError, "Error reading Sourcegraph license key", "Site admins may check the logs for more information. Update the license key in the [**site configuration**](/site-admin/configuration).")
				})
				return
			}
			if info != nil && info.IsExpired() {
				siteadminOrHandler(func() {
					enforcement.WriteSubscriptionErrorResponse(w, http.StatusForbidden, "Sourcegraph license expired", "To continue using Sourcegraph, a site admin must renew the Sourcegraph license (or downgrade to only using Sourcegraph Free features). Update the license key in the [**site configuration**](/site-admin/configuration).")
				})
				return
			}

			next.ServeHTTP(w, r)
		})
	}

	go func() {
		t := time.NewTicker(5 * time.Second)
		for range t.C {
			allowAccessByDefault, authzProviders, _, _, _ := eiauthz.ProvidersFromConfig(ctx, conf.Get(), extsvcStore, db)
			authz.SetProviders(allowAccessByDefault, authzProviders)
		}
	}()

	enterpriseServices.AuthzResolver = resolvers.NewResolver(observationCtx, db)
	return nil
}
