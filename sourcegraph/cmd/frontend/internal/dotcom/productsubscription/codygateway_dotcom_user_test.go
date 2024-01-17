package productsubscription_test

import (
	"context"
	"math"
	"testing"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/dotcom/productsubscription"
	"github.com/sourcegraph/sourcegraph/internal/accesstoken"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/audit/audittest"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestCodyGatewayDotcomUserResolver(t *testing.T) {
	chatOverrideLimit := 200
	codeOverrideLimit := 400

	tru := true
	cfg := &conf.Unified{
		SiteConfiguration: schema.SiteConfiguration{
			CodyEnabled: &tru,
			LicenseKey:  "asdf",
			Completions: &schema.Completions{
				Provider:                         "sourcegraph",
				PerUserCodeCompletionsDailyLimit: 20,
				PerUserDailyLimit:                10,
			},
		},
	}
	conf.Mock(cfg)
	defer func() {
		conf.Mock(nil)
	}()

	ctx := context.Background()
	db := database.NewDB(logtest.Scoped(t), dbtest.NewDB(t))

	// User with default rate limits
	adminUser, err := db.Users().Create(ctx, database.NewUser{Username: "admin", EmailIsVerified: true, Email: "admin@test.com"})
	require.NoError(t, err)

	// Verified User with default rate limits
	verifiedUser, err := db.Users().Create(ctx, database.NewUser{Username: "verified", EmailIsVerified: true, Email: "verified@test.com"})
	require.NoError(t, err)

	// Unverified User with default rate limits
	unverifiedUser, err := db.Users().Create(ctx, database.NewUser{Username: "unverified", EmailIsVerified: false, Email: "christopher.warwick@sourcegraph.com", EmailVerificationCode: "CODE"})
	require.NoError(t, err)

	// User with rate limit overrides
	overrideUser, err := db.Users().Create(ctx, database.NewUser{Username: "override", EmailIsVerified: true, Email: "override@test.com"})
	require.NoError(t, err)
	err = db.Users().SetChatCompletionsQuota(context.Background(), overrideUser.ID, pointers.Ptr(chatOverrideLimit))
	require.NoError(t, err)
	err = db.Users().SetCodeCompletionsQuota(context.Background(), overrideUser.ID, pointers.Ptr(codeOverrideLimit))
	require.NoError(t, err)

	tests := []struct {
		name        string
		user        *types.User
		wantChat    graphqlbackend.BigInt
		wantCode    graphqlbackend.BigInt
		wantEnabled bool
	}{
		{
			name:        "admin user",
			user:        adminUser,
			wantChat:    graphqlbackend.BigInt(cfg.Completions.PerUserDailyLimit),
			wantCode:    graphqlbackend.BigInt(cfg.Completions.PerUserCodeCompletionsDailyLimit),
			wantEnabled: true,
		},
		{
			name:        "verified user default limits",
			user:        verifiedUser,
			wantChat:    graphqlbackend.BigInt(cfg.Completions.PerUserDailyLimit),
			wantCode:    graphqlbackend.BigInt(cfg.Completions.PerUserCodeCompletionsDailyLimit),
			wantEnabled: true,
		},
		{
			name:        "unverified user",
			user:        unverifiedUser,
			wantChat:    0,
			wantCode:    0,
			wantEnabled: false,
		},
		{
			name:        "override user",
			user:        overrideUser,
			wantChat:    graphqlbackend.BigInt(chatOverrideLimit),
			wantCode:    graphqlbackend.BigInt(codeOverrideLimit),
			wantEnabled: true,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {

			// Create an admin context to use for the request
			adminContext := actor.WithActor(context.Background(), actor.FromActualUser(adminUser))

			// Generate a dotcom api Token for the test user
			_, dotcomToken, err := db.AccessTokens().Create(context.Background(), test.user.ID, []string{authz.ScopeUserAll}, test.name, test.user.ID)
			require.NoError(t, err)
			// convert token into a gateway token
			gatewayToken, err := accesstoken.GenerateDotcomUserGatewayAccessToken(dotcomToken)
			require.NoError(t, err)

			logger, exportLogs := logtest.Captured(t)

			// Make request from the admin checking the test user's token
			r := productsubscription.CodyGatewayDotcomUserResolver{Logger: logger, DB: db}
			userResolver, err := r.CodyGatewayDotcomUserByToken(adminContext, &graphqlbackend.CodyGatewayUsersByAccessTokenArgs{Token: gatewayToken})
			require.NoError(t, err)

			chat, err := userResolver.CodyGatewayAccess().ChatCompletionsRateLimit(adminContext)
			require.NoError(t, err)
			if chat != nil {
				require.Equal(t, test.wantChat, chat.Limit())
			} else {
				require.False(t, test.wantEnabled) // If there is no limit make sure it's expected to be disabled
			}

			code, err := userResolver.CodyGatewayAccess().CodeCompletionsRateLimit(adminContext)
			require.NoError(t, err)
			if chat != nil {
				require.Equal(t, test.wantCode, code.Limit())
			} else {
				require.False(t, test.wantEnabled) // If there is no limit make sure it's expected to be disabled
			}

			assert.Equal(t, test.wantEnabled, userResolver.CodyGatewayAccess().Enabled())

			// A user was resolved in this test case, we should have an audit log
			assert.True(t, exportLogs().Contains(func(l logtest.CapturedLog) bool {
				fields, ok := audittest.ExtractAuditFields(l)
				if !ok {
					return ok
				}
				return fields.Entity == "dotcom-codygatewayuser" && fields.Action == "access"
			}))
		})
	}
}

func TestCodyGatewayDotcomUserResolverUserNotFound(t *testing.T) {
	ctx := context.Background()
	db := database.NewDB(logtest.Scoped(t), dbtest.NewDB(t))

	// admin user to make request
	adminUser, err := db.Users().Create(ctx, database.NewUser{Username: "admin", EmailIsVerified: true, Email: "admin@test.com"})
	require.NoError(t, err)

	// Create an admin context to use for the request
	adminContext := actor.WithActor(context.Background(), actor.FromActualUser(adminUser))

	r := productsubscription.CodyGatewayDotcomUserResolver{Logger: logtest.Scoped(t), DB: db}
	_, err = r.CodyGatewayDotcomUserByToken(adminContext, &graphqlbackend.CodyGatewayUsersByAccessTokenArgs{Token: "NOT_A_TOKEN"})

	_, got := err.(productsubscription.ErrDotcomUserNotFound)
	assert.True(t, got, "should have error type ErrDotcomUserNotFound")
}

func TestCodyGatewayDotcomUserResolverRequestAccess(t *testing.T) {
	ctx := context.Background()
	db := database.NewDB(logtest.Scoped(t), dbtest.NewDB(t))

	// Admin
	adminUser, err := db.Users().Create(ctx, database.NewUser{Username: "admin", EmailIsVerified: true, Email: "admin@test.com"})
	require.NoError(t, err)

	// Not Admin with feature flag
	notAdminUser, err := db.Users().Create(ctx, database.NewUser{Username: "verified", EmailIsVerified: true, Email: "verified@test.com"})
	require.NoError(t, err)

	// No admin, no feature flag
	noAccessUser, err := db.Users().Create(ctx, database.NewUser{Username: "nottheone", EmailIsVerified: true, Email: "nottheone@test.com"})
	require.NoError(t, err)

	// cody user
	codyUser, err := db.Users().Create(ctx, database.NewUser{Username: "cody", EmailIsVerified: true, Email: "cody@test.com"})
	require.NoError(t, err)
	// Generate a token for the cody user
	_, codyUserApiToken, err := db.AccessTokens().Create(context.Background(), codyUser.ID, []string{authz.ScopeUserAll}, "cody", codyUser.ID)
	require.NoError(t, err)
	codyUserGatewayToken, err := accesstoken.GenerateDotcomUserGatewayAccessToken(codyUserApiToken)
	require.NoError(t, err)

	// Create a feature flag override entry for the notAdminUser.
	_, err = db.FeatureFlags().CreateBool(context.Background(), "product-subscriptions-reader-service-account", false)
	require.NoError(t, err)
	_, err = db.FeatureFlags().CreateOverride(context.Background(), &featureflag.Override{FlagName: "product-subscriptions-reader-service-account", Value: true, UserID: &notAdminUser.ID})
	require.NoError(t, err)

	tests := []struct {
		name    string
		user    *types.User
		wantErr error
	}{
		{
			name:    "admin user",
			user:    adminUser,
			wantErr: nil,
		},
		{
			name:    "service account",
			user:    notAdminUser,
			wantErr: nil,
		},
		{
			name:    "not admin or service account user",
			user:    noAccessUser,
			wantErr: auth.ErrMustBeSiteAdmin,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {

			// Create a request context from the user
			userContext := actor.WithActor(context.Background(), actor.FromActualUser(test.user))

			// Make request from the test user
			r := productsubscription.CodyGatewayDotcomUserResolver{Logger: logtest.Scoped(t), DB: db}
			_, err := r.CodyGatewayDotcomUserByToken(userContext, &graphqlbackend.CodyGatewayUsersByAccessTokenArgs{Token: codyUserGatewayToken})

			require.ErrorIs(t, err, test.wantErr)
		})
	}
}

func TestCodyGatewayCompletionsRateLimit(t *testing.T) {
	ctx := context.Background()
	db := database.NewDB(logtest.Scoped(t), dbtest.NewDB(t))

	override := 20
	perUserDailyLimit := 30
	perCommunityUserChatMonthlyLLMRequestLimit := 40
	perProUserChatDailyLLMRequestLimit := 50
	oneDayInSeconds := int32(60 * 60 * 24)

	// Create feature flags
	limitsExceeded := "rate-limits-exceeded-for-testing"
	_, err := db.FeatureFlags().CreateBool(ctx, limitsExceeded, false)
	require.NoError(t, err)

	tru := true
	cfg := &conf.Unified{
		SiteConfiguration: schema.SiteConfiguration{
			CodyEnabled: &tru,
			LicenseKey:  "asdf",
			Completions: &schema.Completions{
				Provider: "sourcegraph",
				PerCommunityUserChatMonthlyLLMRequestLimit: perCommunityUserChatMonthlyLLMRequestLimit,
				PerProUserChatDailyLLMRequestLimit:         perProUserChatDailyLLMRequestLimit,
				PerUserDailyLimit:                          perUserDailyLimit,
			},
		},
	}
	conf.Mock(cfg)
	defer func() {
		conf.Mock(nil)
	}()

	// User with an override
	userWithOverrides, err := db.Users().Create(ctx, database.NewUser{Username: "override", EmailIsVerified: true, Email: "override@test.com"})
	require.NoError(t, err)
	err = db.Users().SetChatCompletionsQuota(context.Background(), userWithOverrides.ID, pointers.Ptr(override))
	require.NoError(t, err)

	// Cody SSC - Free user
	sscFreeUser, err := db.Users().Create(ctx, database.NewUser{Username: "ssc-free", EmailIsVerified: true, Email: "ssc-free@test.com"})
	require.NoError(t, err)

	// Cody SSC - Pro user
	sscProUser, err := db.Users().Create(ctx, database.NewUser{Username: "ssc-pro", EmailIsVerified: true, Email: "ssc-pro@test.com"})
	require.NoError(t, err)
	err = db.Users().ChangeCodyPlan(ctx, sscProUser.ID, true)
	require.NoError(t, err)

	// Rate limited Cody SSC - Free user
	rateLimitsExceededFreeUser, err := db.Users().Create(ctx, database.NewUser{Username: "free-limited", EmailIsVerified: true, Email: "free-limited@test.com"})
	require.NoError(t, err)
	_, err = db.FeatureFlags().CreateOverride(ctx, &featureflag.Override{FlagName: limitsExceeded, Value: true, UserID: &rateLimitsExceededFreeUser.ID})
	require.NoError(t, err)

	// Rate limited Cody SSC - Pro user
	rateLimitsExceededProUser, err := db.Users().Create(ctx, database.NewUser{Username: "pro-limited", EmailIsVerified: true, Email: "pro-limited@test.com"})
	require.NoError(t, err)
	_, err = db.FeatureFlags().CreateOverride(ctx, &featureflag.Override{FlagName: limitsExceeded, Value: true, UserID: &rateLimitsExceededProUser.ID})
	require.NoError(t, err)

	tests := []struct {
		name                            string
		user                            *types.User
		wantChatLimit                   graphqlbackend.BigInt
		wantChatLimitInterval           int32
		wantCodeCompletionLimit         graphqlbackend.BigInt
		wantCodeCompletionLimitInterval int32
	}{
		{
			name:                            "override",
			user:                            userWithOverrides,
			wantChatLimit:                   graphqlbackend.BigInt(override),
			wantChatLimitInterval:           oneDayInSeconds,
			wantCodeCompletionLimit:         graphqlbackend.BigInt(0),
			wantCodeCompletionLimitInterval: oneDayInSeconds,
		},
		{
			name:                            "ssc-free",
			user:                            sscFreeUser,
			wantChatLimit:                   graphqlbackend.BigInt(perCommunityUserChatMonthlyLLMRequestLimit),
			wantChatLimitInterval:           oneDayInSeconds * 30,
			wantCodeCompletionLimit:         graphqlbackend.BigInt(0),
			wantCodeCompletionLimitInterval: oneDayInSeconds,
		},
		{
			name:                            "ssc-pro",
			user:                            sscProUser,
			wantChatLimit:                   graphqlbackend.BigInt(perProUserChatDailyLLMRequestLimit),
			wantChatLimitInterval:           oneDayInSeconds,
			wantCodeCompletionLimit:         graphqlbackend.BigInt(0),
			wantCodeCompletionLimitInterval: oneDayInSeconds,
		},
		{
			name:                            "free-limited",
			user:                            rateLimitsExceededFreeUser,
			wantChatLimit:                   graphqlbackend.BigInt(1),
			wantChatLimitInterval:           math.MaxInt32,
			wantCodeCompletionLimit:         graphqlbackend.BigInt(1),
			wantCodeCompletionLimitInterval: math.MaxInt32,
		},
		{
			name:                            "pro-limited",
			user:                            rateLimitsExceededProUser,
			wantChatLimit:                   graphqlbackend.BigInt(1),
			wantChatLimitInterval:           math.MaxInt32,
			wantCodeCompletionLimit:         graphqlbackend.BigInt(1),
			wantCodeCompletionLimitInterval: math.MaxInt32,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// Make user an admin
			err = db.Users().SetIsSiteAdmin(ctx, test.user.ID, true)
			user, err := db.Users().GetByID(ctx, test.user.ID)
			require.NoError(t, err)

			// Create resolver and get user
			_, apiToken, err := db.AccessTokens().Create(ctx, user.ID, []string{authz.ScopeUserAll}, "test", user.ID)
			require.NoError(t, err)
			gatewayToken, err := accesstoken.GenerateDotcomUserGatewayAccessToken(apiToken)
			require.NoError(t, err)

			// Create a request context from the user
			userContext := featureflag.WithFlags(actor.WithActor(ctx, actor.FromActualUser(user)), db.FeatureFlags())
			r := productsubscription.CodyGatewayDotcomUserResolver{Logger: logtest.Scoped(t), DB: db}
			gatewayUser, err := r.CodyGatewayDotcomUserByToken(userContext, &graphqlbackend.CodyGatewayUsersByAccessTokenArgs{Token: gatewayToken})
			require.NoError(t, err)

			access := gatewayUser.CodyGatewayAccess()
			rateLimit, err := access.ChatCompletionsRateLimit(userContext)
			require.NoError(t, err)
			require.Equal(t, test.wantChatLimit, rateLimit.Limit())
			require.Equal(t, test.wantChatLimitInterval, rateLimit.IntervalSeconds())

			rateLimit, err = access.CodeCompletionsRateLimit(userContext)
			require.NoError(t, err)
			require.Equal(t, test.wantCodeCompletionLimit, rateLimit.Limit())
			require.Equal(t, test.wantCodeCompletionLimitInterval, rateLimit.IntervalSeconds())
		})
	}
}
