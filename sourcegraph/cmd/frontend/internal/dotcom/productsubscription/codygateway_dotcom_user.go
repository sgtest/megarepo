package productsubscription

import (
	"context"
	"fmt"
	"math"

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

func (r codyUserGatewayAccessResolver) EmbeddingsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	// If the user isn't enabled return no rate limit
	if !r.Enabled() {
		return nil, nil
	}

	rateLimit := licensing.CodyGatewayRateLimit{
		AllowedModels:   []string{"openai/text-embedding-ada-002"},
		Limit:           int64(20 * tokensPerDollar),
		IntervalSeconds: math.MaxInt32,
	}

	return &codyGatewayRateLimitResolver{
		actorID:     r.user.Username,
		actorSource: codygateway.ActorSourceDotcomUser,
		source:      graphqlbackend.CodyGatewayRateLimitSourcePlan,
		v:           rateLimit,
	}, nil
}

func getCompletionsRateLimit(ctx context.Context, db database.DB, userID int32, scope types.CompletionsFeature) (licensing.CodyGatewayRateLimit, graphqlbackend.CodyGatewayRateLimitSource, error) {
	var limit *int
	var err error
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
	if limit == nil {
		source = graphqlbackend.CodyGatewayRateLimitSourcePlan
		// Otherwise, fall back to the global limit.
		cfg := conf.GetCompletionsConfig(conf.Get().SiteConfig())
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
			return licensing.CodyGatewayRateLimit{}, graphqlbackend.CodyGatewayRateLimitSourcePlan, errors.Newf("unknown scope: %s", scope)
		}
	}
	if limit == nil {
		limit = pointers.Ptr(0)
	}
	return licensing.CodyGatewayRateLimit{
		AllowedModels:   allowedModels(scope),
		Limit:           int64(*limit),
		IntervalSeconds: 86400, // Daily limit TODO(davejrt)
	}, source, nil
}

func allowedModels(scope types.CompletionsFeature) []string {
	switch scope {
	case types.CompletionsFeatureChat:
		return []string{"anthropic/claude-v1", "anthropic/claude-2", "anthropic/claude-instant-v1", "anthropic/claude-instant-1"}
	case types.CompletionsFeatureCode:
		return []string{"anthropic/claude-instant-v1", "anthropic/claude-instant-1"}
	default:
		return []string{}
	}
}
