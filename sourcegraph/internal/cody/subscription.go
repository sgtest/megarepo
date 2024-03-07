package cody

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/ssc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// featureFlagUseSCCForSubscription determines if we should attempt to lookup subscription data from SSC.
const featureFlagUseSCCForSubscription = "use-ssc-for-cody-subscription-on-web"

// featureFlagCodyProTrialEnded indicates if the Cody Pro "Free Trial"  has ended.
// (Enabling users to use Cody Pro for free for 3-months starting in late Q4'2023.)
const featureFlagCodyProTrialEnded = "cody-pro-trial-ended"

type UserSubscriptionPlan string

const (
	UserSubscriptionPlanFree UserSubscriptionPlan = "FREE"
	UserSubscriptionPlanPro  UserSubscriptionPlan = "PRO"
)

type UserSubscription struct {
	// Status is the current status of the subscription. "pending" means the user has no Cody Pro subscription.
	// "pending" subscription will be removed post Feb 15, 2024. It is required to support users who have opted for
	// Cody Pro Trial on dotcom, but have not entered payment information on SSC.
	// (So they don't have an SSC backed subscription, but we need to act like they do, until 2/15.)
	Status ssc.SubscriptionStatus
	// Plan is the plan the user is subscribed to.
	Plan UserSubscriptionPlan
	// ApplyProRateLimits indicates the user should be given higher rate limits
	// for Cody and related functionality. (Use this value instead of checking
	// the subscription status for simplicity.)
	ApplyProRateLimits bool
	// CurrentPeriodStartAt is the start date of the current billing cycle.
	CurrentPeriodStartAt time.Time
	// CurrentPeriodEndAt is the end date of the current billing cycle.
	// IMPORTANT: This may be IN THE PAST. e.g. if the subscription was
	// canceled, this will be when the subscription ended.
	CurrentPeriodEndAt time.Time
	// CancelAtPeriodEnd flags whether or not a subscription will automatically cancel at the end
	// of the current billing cycle, or if it will renew.
	CancelAtPeriodEnd bool
}

// consolidateSubscriptionDetails merges the subscription data available on dotcom and SCC.
// This is needed while transitioning to use SCC as the source of truth for all subscription data. (Which should happen ~Q1/2024.)
// TODO[sourcegraph#59785]: Update dotcom to use SSC as the source of truth for all subscription data.
func consolidateSubscriptionDetails(ctx context.Context, user types.User, subscription *ssc.Subscription) (*UserSubscription, error) {
	// If subscription information is available from SSC, we use that.
	// And just ignore what is stored in dotcom. (Since they've already
	// been migrated so to speak.)
	if subscription != nil && subscription.Status != ssc.SubscriptionStatusCanceled {
		currentPeriodStart, err := time.Parse(time.RFC3339, subscription.CurrentPeriodStart)
		if err != nil {
			return nil, err
		}

		currentPeriodEnd, err := time.Parse(time.RFC3339, subscription.CurrentPeriodEnd)
		if err != nil {
			return nil, err
		}

		applyProRateLimits := subscription.Status == ssc.SubscriptionStatusActive || subscription.Status == ssc.SubscriptionStatusPastDue || (subscription.Status == ssc.SubscriptionStatusTrialing && !subscription.CancelAtPeriodEnd)

		return &UserSubscription{
			Status:               subscription.Status,
			Plan:                 UserSubscriptionPlanPro,
			ApplyProRateLimits:   applyProRateLimits,
			CurrentPeriodStartAt: currentPeriodStart,
			CurrentPeriodEndAt:   currentPeriodEnd,
			CancelAtPeriodEnd:    subscription.CancelAtPeriodEnd,
		}, nil
	}

	// If the user doesn't have a subscription in the SSC backend or it is cancelled, then we need
	// synthesize one using the data available on dotcom.
	currentPeriodStartAt, currentPeriodEndAt, err := preSSCReleaseCurrentPeriodDateRange(ctx, user, subscription)
	if err != nil {
		return nil, err
	}

	if subscription != nil && subscription.Status == ssc.SubscriptionStatusCanceled {
		return &UserSubscription{
			Status:               ssc.SubscriptionStatusActive,
			Plan:                 UserSubscriptionPlanFree,
			ApplyProRateLimits:   false,
			CurrentPeriodStartAt: *currentPeriodStartAt,
			CurrentPeriodEndAt:   *currentPeriodEndAt,
			CancelAtPeriodEnd:    false,
		}, nil
	}

	// Whether or not the Cody Pro free trial offer is still running.
	codyProTrialEnded := featureflag.FromContext(ctx).GetBoolOr(featureFlagCodyProTrialEnded, false)

	if user.CodyProEnabledAt != nil {
		return &UserSubscription{
			Status:               ssc.SubscriptionStatusPending,
			Plan:                 UserSubscriptionPlanPro,
			ApplyProRateLimits:   !codyProTrialEnded,
			CurrentPeriodStartAt: *currentPeriodStartAt,
			CurrentPeriodEndAt:   *currentPeriodEndAt,
			CancelAtPeriodEnd:    false,
		}, nil
	}

	return &UserSubscription{
		Status:               ssc.SubscriptionStatusPending,
		Plan:                 UserSubscriptionPlanFree,
		ApplyProRateLimits:   false,
		CurrentPeriodStartAt: *currentPeriodStartAt,
		CurrentPeriodEndAt:   *currentPeriodEndAt,
		CancelAtPeriodEnd:    false,
	}, nil
}

// getSAMSAccountIDsForUser returns the user's SAMS account ID if available.
//
// If the user has not associated a SAMS identity with their dotcom user account,
// will return ("", nil). After we migrate all dotcom user accounts to SAMS, that
// should no longer be possible.
func getSAMSAccountIDsForUser(ctx context.Context, db database.DB, dotcomUserID int32) ([]string, error) {
	// NOTE: We hard-code this to look for the SAMS-prod environment, meaning there isn't a way
	// to test dotcom pulling subscription data from a local SAMS/SSC instance. To support that
	// we'd need to make the SAMSHostname configurable. (Or somehow identify which OIDC provider
	// is SAMS.)
	oidcAccounts, err := db.UserExternalAccounts().List(ctx, database.ExternalAccountsListOptions{
		UserID:      dotcomUserID,
		ServiceType: "openidconnect",
		ServiceID:   fmt.Sprintf("https://%s", ssc.GetSAMSHostName()),
		LimitOffset: &database.LimitOffset{
			Limit: 1,
		},
	})
	if err != nil {
		return []string{}, errors.Wrap(err, "listing external accounts")
	}

	var ids []string

	for _, account := range oidcAccounts {
		ids = append(ids, account.AccountID)
	}

	return ids, nil
}

// SubscriptionForUser returns the user's Cody subscription details.
func SubscriptionForUser(ctx context.Context, db database.DB, user types.User) (*UserSubscription, error) {
	samsAccountIDs, err := getSAMSAccountIDsForUser(ctx, db, user.ID)
	if err != nil {
		return nil, errors.Wrap(err, "fetching user's SAMS account ID")
	}

	if len(samsAccountIDs) == 0 {
		return consolidateSubscriptionDetails(ctx, user, nil)
	}

	// NOTE(naman): As part of #inc-284-plg-users-paying-for-and-being-billed-for-pro-without-being-upgrade
	// it is noted that a user can have multiple SAMS accounts associated with their dotcom account.
	// Originally we only fetchted subscription data from the first SAMS account associated with the user.
	// But we are now fetching subscription data from all SAMS accounts associated with the user, and using
	// the one with active subscription. This is only a TEMPORARY solution to mititgate the incident.
	// MUST be removed/fixed once we have a proper solution in place as part of #60912.
	var subscriptions []*ssc.Subscription
	for _, samsAccountID := range samsAccountIDs {
		// While developing the SSC backend, we only fetch subscription data for users based on a flag.
		var subscription *ssc.Subscription
		useSCCForSubscriptionData := featureflag.FromContext(ctx).GetBoolOr(featureFlagUseSCCForSubscription, false)

		if samsAccountID != "" && useSCCForSubscriptionData {
			sscClient, err := getSSCClient()
			if err != nil {
				return nil, err
			}

			subscription, err = sscClient.FetchSubscriptionBySAMSAccountID(ctx, samsAccountID)
			if err != nil {
				return nil, errors.Wrap(err, "fetching subscription from SSC")
			}
		}

		if subscription != nil {
			subscriptions = append(subscriptions, subscription)

			if subscription.Status == ssc.SubscriptionStatusActive {
				return consolidateSubscriptionDetails(ctx, user, subscription)
			}
		}
	}

	if len(subscriptions) == 0 {
		return consolidateSubscriptionDetails(ctx, user, nil)
	}

	return consolidateSubscriptionDetails(ctx, user, subscriptions[0])
}

// SubscriptionForSAMSAccountID returns the user's Cody subscription details associated with the given SAMS account ID.
func SubscriptionForSAMSAccountID(ctx context.Context, db database.DB, user types.User, samsAccountID string) (*UserSubscription, error) {
	if samsAccountID == "" {
		return consolidateSubscriptionDetails(ctx, user, nil)
	}

	var subscription *ssc.Subscription
	sscClient, err := getSSCClient()
	if err != nil {
		return nil, err
	}

	subscription, err = sscClient.FetchSubscriptionBySAMSAccountID(ctx, samsAccountID)
	if err != nil {
		return nil, errors.Wrap(err, "fetching subscription from SSC")
	}

	return consolidateSubscriptionDetails(ctx, user, subscription)
}

// getSSCClient returns a self-service Cody API client. We only do this once so that the stateless client
// can persist in memory longer, so we can benefit from the underlying HTTP client only needing to reissue
// SAMS access tokens when needed, rather than minting a new token for every request.
//
// BUG: If the SAMS configuration is added or changed during the lifetime of the this process, the returned
// client will be invalid. (As it would be using the original SAMS client configuration data.) The process
// will need to be restarted to correct this situation.
var getSSCClient = sync.OnceValues[ssc.Client, error](func() (ssc.Client, error) {
	return ssc.NewClient()
})
