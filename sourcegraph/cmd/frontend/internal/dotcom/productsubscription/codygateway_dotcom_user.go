package productsubscription

import (
	"context"
	"fmt"
	"math"

	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/internal/audit"
	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/licensing"
	dbtypes "github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

const auditEntityDotcomCodyGatewayUser = "dotcom-codygatewayuser"

type ErrDotcomUserNotFound struct {
	err error
}

func (e ErrDotcomUserNotFound) Error() string {
	if e.err == nil {
		return "dotcom user not found"
	}
	return fmt.Sprintf("dotcom user not found: %v", e.err)
}

func (e ErrDotcomUserNotFound) Extensions() map[string]any {
	return map[string]any{"code": codygateway.GQLErrCodeDotcomUserNotFound}
}

// CodyGatewayDotcomUserResolver implements the GraphQL Query and Mutation fields related to Cody gateway users.
type CodyGatewayDotcomUserResolver struct {
	Logger log.Logger
	DB     database.DB
}

func (r CodyGatewayDotcomUserResolver) CodyGatewayDotcomUserByToken(ctx context.Context, args *graphqlbackend.CodyGatewayUsersByAccessTokenArgs) (graphqlbackend.CodyGatewayUser, error) {
	// 🚨 SECURITY: Only site admins or the service accounts may check users.
	grantReason, err := serviceAccountOrSiteAdmin(ctx, r.DB, false)
	if err != nil {
		return nil, err
	}

	dbTokens := newDBTokens(r.DB)
	userID, err := dbTokens.LookupDotcomUserIDByAccessToken(ctx, args.Token)
	if err != nil {
		if errcode.IsNotFound(err) {
			return nil, ErrDotcomUserNotFound{err}
		}
		return nil, err
	}

	// 🚨 SECURITY: Record access with the resolved user ID
	audit.Log(ctx, r.Logger, audit.Record{
		Entity: auditEntityDotcomCodyGatewayUser,
		Action: "access",
		Fields: []log.Field{
			log.String("grant_reason", grantReason),
			log.Int("accessed_user_id", userID),
		},
	})

	user, err := r.DB.Users().GetByID(ctx, int32(userID))
	if err != nil {
		if errcode.IsNotFound(err) {
			return nil, ErrDotcomUserNotFound{err}
		}
		return nil, err
	}
	verified, err := r.DB.UserEmails().HasVerifiedEmail(ctx, user.ID)
	if err != nil {
		return nil, err
	}
	return &dotcomCodyUserResolver{
		db:            r.DB,
		user:          user,
		verifiedEmail: verified,
	}, nil

}

type dotcomCodyUserResolver struct {
	db            database.DB
	user          *dbtypes.User
	verifiedEmail bool
}

func (u *dotcomCodyUserResolver) Username() string {
	return u.user.Username
}

func (u *dotcomCodyUserResolver) ID() graphql.ID {
	return relay.MarshalID("User", u.user.ID)
}

func (u *dotcomCodyUserResolver) CodyGatewayAccess() graphqlbackend.CodyGatewayAccess {
	return &codyUserGatewayAccessResolver{
		db:            u.db,
		user:          u.user,
		verifiedEmail: u.verifiedEmail,
	}
}

type codyUserGatewayAccessResolver struct {
	db            database.DB
	user          *dbtypes.User
	verifiedEmail bool
}

func (r codyUserGatewayAccessResolver) Enabled() bool { return r.user.SiteAdmin || r.verifiedEmail }

func (r codyUserGatewayAccessResolver) ChatCompletionsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	// If the user isn't enabled return no rate limit
	if !r.Enabled() {
		return nil, nil
	}
	rateLimit, rateLimitSource, err := getCompletionsRateLimit(ctx, r.db, r.user.ID, types.CompletionsFeatureChat)
	if err != nil {
		return nil, err
	}

	return &codyGatewayRateLimitResolver{
		feature:     types.CompletionsFeatureChat,
		actorID:     r.user.Username,
		actorSource: codygateway.ActorSourceDotcomUser,
		source:      rateLimitSource,
		v:           rateLimit,
	}, nil
}

func (r codyUserGatewayAccessResolver) CodeCompletionsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	// If the user isn't enabled return no rate limit
	if !r.Enabled() {
		return nil, nil
	}

	rateLimit, rateLimitSource, err := getCompletionsRateLimit(ctx, r.db, r.user.ID, types.CompletionsFeatureCode)
	if err != nil {
		return nil, err
	}

	return &codyGatewayRateLimitResolver{
		feature:     types.CompletionsFeatureCode,
		actorID:     r.user.Username,
		actorSource: codygateway.ActorSourceDotcomUser,
		source:      rateLimitSource,
		v:           rateLimit,
	}, nil
}

const tokensPerDollar = int(1 / (0.0001 / 1_000))
const oneDayInSeconds = int32(60 * 60 * 24)

// oneMonthInSeconds is a bad approximation. Our logic in Cody Gateway
// is "till the Nth day of the next month", so this is basically a magic number,
// and we shouldn't use this value as a duration.
const oneMonthInSeconds = oneDayInSeconds * 30

func (r codyUserGatewayAccessResolver) EmbeddingsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	// If the user isn't enabled return no rate limit
	if !r.Enabled() {
		return nil, nil
	}

	rateLimit, err := getEmbeddingsRateLimit(ctx, r.db, r.user.ID)
	if err != nil {
		return nil, err
	}

	return &codyGatewayRateLimitResolver{
		actorID:     r.user.Username,
		actorSource: codygateway.ActorSourceDotcomUser,
		source:      graphqlbackend.CodyGatewayRateLimitSourcePlan,
		v:           rateLimit,
	}, nil
}

func getEmbeddingsRateLimit(ctx context.Context, db database.DB, userID int32) (licensing.CodyGatewayRateLimit, error) {
	// Hard-coded defaults: 200M tokens for life
	limit := int64(20 * tokensPerDollar)
	intervalSeconds := int32(math.MaxInt32)

	// Apply self-serve limits if available
	cfg := conf.GetEmbeddingsConfig(conf.Get().SiteConfig())
	if cfg != nil && featureflag.FromContext(ctx).GetBoolOr("cody-pro", false) {
		user, err := db.Users().GetByID(ctx, userID)
		if err != nil {
			return licensing.CodyGatewayRateLimit{}, err
		}
		intervalSeconds = oneMonthInSeconds
		isProUser := user.CodyProEnabledAt != nil
		if isProUser {
			if cfg.PerProUserEmbeddingsMonthlyLimit > 0 {
				limit = int64(cfg.PerProUserEmbeddingsMonthlyLimit)
			}
		} else {
			if cfg.PerCommunityUserEmbeddingsMonthlyLimit > 0 {
				limit = int64(cfg.PerCommunityUserEmbeddingsMonthlyLimit)
			}
		}
	}

	// Apply override for testing if set
	if featureflag.FromContext(ctx).GetBoolOr("rate-limits-exceeded-for-testing", false) {
		limit = 1
	}

	return licensing.CodyGatewayRateLimit{
		AllowedModels:   []string{"openai/text-embedding-ada-002"},
		Limit:           limit,
		IntervalSeconds: intervalSeconds,
	}, nil
}

// getCompletionsRateLimit returns a rate limit for the given user and feature.
func getCompletionsRateLimit(ctx context.Context, db database.DB, userID int32, scope types.CompletionsFeature) (licensing.CodyGatewayRateLimit, graphqlbackend.CodyGatewayRateLimitSource, error) {
	var limit *int
	var err error

	isCodyProEnabled := featureflag.FromContext(ctx).GetBoolOr("cody-pro", false)

	// Apply override for testing if set.
	if featureflag.FromContext(ctx).GetBoolOr("rate-limits-exceeded-for-testing", false) {
		return licensing.CodyGatewayRateLimit{
			// For this special tester user, just allow all models a Pro user can get.
			AllowedModels:   allowedModels(scope, true, true),
			Limit:           1,
			IntervalSeconds: math.MaxInt32,
		}, graphqlbackend.CodyGatewayRateLimitSourceOverride, nil
	}

	// Apply overrides first.
	source := graphqlbackend.CodyGatewayRateLimitSourceOverride
	switch scope {
	case types.CompletionsFeatureChat:
		limit, err = db.Users().GetChatCompletionsQuota(ctx, userID)
	case types.CompletionsFeatureCode:
		limit, err = db.Users().GetCodeCompletionsQuota(ctx, userID)
	default:
		return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, errors.Newf("unknown scope: %s", scope)
	}
	if err != nil {
		return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, err
	}

	// If there's no override, check the self-serve limits.
	cfg := conf.GetCompletionsConfig(conf.Get().SiteConfig())
	intervalSeconds := oneDayInSeconds
	// We may not update the limit, but we should check the models
	user, err := db.Users().GetByID(ctx, userID)
	if err != nil {
		return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, err
	}
	isProUser := user.CodyProEnabledAt != nil
	models := allowedModels(scope, isCodyProEnabled, isProUser)
	if limit == nil && cfg != nil && isCodyProEnabled {
		source = graphqlbackend.CodyGatewayRateLimitSourcePlan
		user, err := db.Users().GetByID(ctx, userID)
		if err != nil {
			return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, err
		}
		isProUser := user.CodyProEnabledAt != nil
		// Update the allowed models based on the user's plan.
		models = allowedModels(scope, isCodyProEnabled, isProUser)
		intervalSeconds, limit, err = getSelfServeUsageLimits(scope, isProUser, *cfg)
		if err != nil {
			return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, err
		}
	}

	// Otherwise, fall back to the pre-Cody-GA global limit.
	if limit == nil {
		source = graphqlbackend.CodyGatewayRateLimitSourcePlan
		switch scope {
		case types.CompletionsFeatureChat:
			if cfg != nil && cfg.PerUserDailyLimit > 0 {
				limit = pointers.Ptr(cfg.PerUserDailyLimit)
			}
		case types.CompletionsFeatureCode:
			if cfg != nil && cfg.PerUserCodeCompletionsDailyLimit > 0 {
				limit = pointers.Ptr(cfg.PerUserCodeCompletionsDailyLimit)
			}
		default:
			return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, errors.Newf("unknown scope (dotcom limiting): %s", scope)
		}
	}
	if limit == nil {
		limit = pointers.Ptr(0)
	}
	return licensing.CodyGatewayRateLimit{
		AllowedModels:   models,
		Limit:           int64(*limit),
		IntervalSeconds: intervalSeconds, // Daily limit TODO(davejrt)
	}, source, nil
}

func getSelfServeUsageLimits(scope types.CompletionsFeature, isProUser bool, cfg conftypes.CompletionsConfig) (int32, *int, error) {
	switch scope {
	case types.CompletionsFeatureChat:
		if isProUser {
			if cfg.PerProUserChatDailyLLMRequestLimit > 0 {
				return oneDayInSeconds, pointers.Ptr(cfg.PerProUserChatDailyLLMRequestLimit), nil
			}
		} else {
			if cfg.PerCommunityUserChatMonthlyLLMRequestLimit > 0 {
				return oneMonthInSeconds, pointers.Ptr(cfg.PerCommunityUserChatMonthlyLLMRequestLimit), nil
			}
		}
	case types.CompletionsFeatureCode:
		if isProUser {
			if cfg.PerProUserCodeCompletionsDailyLLMRequestLimit > 0 {
				return oneDayInSeconds, pointers.Ptr(cfg.PerProUserCodeCompletionsDailyLLMRequestLimit), nil
			}
		} else {
			if cfg.PerCommunityUserCodeCompletionsMonthlyLLMRequestLimit > 0 {
				return oneMonthInSeconds, pointers.Ptr(cfg.PerCommunityUserCodeCompletionsMonthlyLLMRequestLimit), nil
			}
		}
	default:
		return 0, nil, errors.Newf("unknown scope (self-serve limiting): %s", scope)
	}
	return oneDayInSeconds, nil, nil
}

func allowedModels(scope types.CompletionsFeature, isCodyProEnabled, isProUser bool) []string {
	switch scope {
	case types.CompletionsFeatureChat:
		if !isCodyProEnabled {
			return []string{
				"anthropic/claude-v1",
				"anthropic/claude-2",
				"anthropic/claude-2.0",
				"anthropic/claude-2.1",
				"anthropic/claude-instant-v1",
				"anthropic/claude-instant-1.2",
				"anthropic/claude-instant-1",
				"openai/gpt-4-1106-preview",
				"fireworks/accounts/fireworks/models/mixtral-8x7b-instruct",
			}
		}

		// When updating the below lists, make sure you also update `isAllowedCustomChatModel` in `chat.go`

		if !isProUser {
			return []string{
				"anthropic/claude-2.0",
				"anthropic/claude-instant-v1",
				"anthropic/claude-instant-1.2",
				"anthropic/claude-instant-1",
			}
		}

		return []string{
			"anthropic/claude-2",
			"anthropic/claude-2.0",
			"anthropic/claude-2.1",
			"anthropic/claude-instant-1.2-cyan",
			"anthropic/claude-instant-1.2",
			"openai/gpt-3.5-turbo",
			"openai/gpt-4-1106-preview",
			"fireworks/accounts/fireworks/models/mixtral-8x7b-instruct",
		}
	case types.CompletionsFeatureCode:
		return []string{
			"anthropic/claude-instant-v1",
			"anthropic/claude-instant-1",
			"fireworks/accounts/fireworks/models/starcoder-7b-w8a16",
			"fireworks/accounts/sourcegraph/models/starcoder-7b",
			"fireworks/accounts/fireworks/models/starcoder-16b-w8a16",
			"fireworks/accounts/sourcegraph/models/starcoder-16b",
		}
	default:
		return []string{}
	}
}
