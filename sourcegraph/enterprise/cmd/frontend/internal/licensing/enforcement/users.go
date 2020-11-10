package enforcement

import (
	"context"
	"fmt"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
)

// NewBeforeCreateUserHook returns a BeforeCreateUserHook closure with the given UsersStore
// that determines whether new user is allowed to be created.
func NewBeforeCreateUserHook(s licensing.UsersStore) func(context.Context) error {
	return func(ctx context.Context) error {
		info, err := licensing.GetConfiguredProductLicenseInfo()
		if err != nil {
			return err
		}
		var licensedUserCount int32
		if info != nil {
			// We prevent creating new users when the license is expired because we do not want
			// all new users to be promoted as site admins automatically until the customer
			// decides to downgrade to Free tier.
			if licensing.EnforceTiers && info.IsExpired() {
				return errcode.NewPresentationError("Unable to create user account: Sourcegraph license expired! No new users can be created. Update the license key in the [**site configuration**](/site-admin/configuration) or downgrade to only using Sourcegraph Free features.")
			}
			licensedUserCount = int32(info.UserCount)
		} else {
			licensedUserCount = licensing.NoLicenseMaximumAllowedUserCount
		}

		// Block creation of a new user beyond the licensed user count (unless true-up is allowed).
		userCount, err := s.Count(ctx)
		if err != nil {
			return err
		}
		// Be conservative and treat 0 as unlimited. We don't plan to intentionally generate
		// licenses with UserCount == 0, but that might result from a bug in license decoding, and
		// we don't want that to immediately disable Sourcegraph instances.
		if licensedUserCount > 0 && int32(userCount) >= licensedUserCount {
			if info != nil && info.HasTag(licensing.TrueUpUserCountTag) {
				log15.Info("Licensed user count exceeded, but license supports true-up and will not block creation of new user. The new user will be retroactively charged for in the next billing period. Contact sales@sourcegraph.com for help.", "activeUserCount", userCount, "licensedUserCount", licensedUserCount)
			} else {
				message := "Unable to create user account: "
				if info == nil {
					message += fmt.Sprintf("a Sourcegraph subscription is required to exceed %d users (this instance now has %d users). Contact Sourcegraph to learn more at https://about.sourcegraph.com/contact/sales.", licensing.NoLicenseMaximumAllowedUserCount, userCount)
				} else {
					message += "the Sourcegraph subscription's maximum user count has been reached. A site admin must upgrade the Sourcegraph subscription to allow for more users. Contact Sourcegraph at https://about.sourcegraph.com/contact/sales."
				}
				return errcode.NewPresentationError(message)
			}
		}

		return nil
	}
}

// NewAfterCreateUserHook returns a AfterCreateUserHook closure that determines whether
// a new user should be promoted to site admin based on the product license.
func NewAfterCreateUserHook() func(context.Context, dbutil.DB, *types.User) error {
	// 🚨 SECURITY: To be extra safe that we never promote any new user to be site admin on Sourcegraph Cloud.
	if !licensing.EnforceTiers || envvar.SourcegraphDotComMode() {
		return nil
	}

	return func(ctx context.Context, tx dbutil.DB, user *types.User) error {
		info, err := licensing.GetConfiguredProductLicenseInfo()
		if err != nil {
			return err
		}

		// Nil info indicates no license, thus Free tier
		if info == nil {
			user.SiteAdmin = true
			// TODO: Use db.Users.SetIsSiteAdmin when it migrated to have `*basestore.Store`
			//  and support `With` method.
			_, err := tx.ExecContext(ctx, "UPDATE users SET site_admin=$1 WHERE id=$2", user.SiteAdmin, user.ID)
			if err != nil {
				return err
			}
		}

		return nil
	}
}

// NewBeforeSetUserIsSiteAdmin returns a BeforeSetUserIsSiteAdmin closure that determines whether
// non-site admin roles are allowed (i.e. revoke site admins) based on the product license.
func NewBeforeSetUserIsSiteAdmin() func(isSiteAdmin bool) error {
	if !licensing.EnforceTiers {
		return nil
	}

	return func(isSiteAdmin bool) error {
		if isSiteAdmin {
			return nil
		}

		info, err := licensing.GetConfiguredProductLicenseInfo()
		if err != nil {
			return err
		}

		if info != nil {
			return nil
		}

		return licensing.NewFeatureNotActivatedError(fmt.Sprintf("The feature %q is not activated because it requires a valid Sourcegraph license. Purchase a Sourcegraph subscription to activate this feature.", "non-site admin roles"))
	}
}
