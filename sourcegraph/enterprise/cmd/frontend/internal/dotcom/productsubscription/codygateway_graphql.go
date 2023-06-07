package productsubscription

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type codyGatewayAccessResolver struct {
	sub *productSubscription
}

func (r codyGatewayAccessResolver) Enabled() bool { return r.sub.v.CodyGatewayAccess.Enabled }

func (r codyGatewayAccessResolver) ChatCompletionsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	if !r.Enabled() {
		return nil, nil
	}

	var rateLimit licensing.CodyGatewayRateLimit

	// Get default access from active license. Call hydrate and access field directly to
	// avoid parsing license key which is done in (*productLicense).Info(), instead just
	// relying on what we know in DB.
	activeLicense, err := r.sub.computeActiveLicense(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "could not get active license")
	}
	var source graphqlbackend.CodyGatewayRateLimitSource
	if activeLicense != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourcePlan
		rateLimit = licensing.NewCodyGatewayChatRateLimit(licensing.PlanFromTags(activeLicense.LicenseTags), activeLicense.LicenseUserCount, activeLicense.LicenseTags)
	}

	// Apply overrides
	rateLimitOverrides := r.sub.v.CodyGatewayAccess
	if rateLimitOverrides.ChatRateLimit.RateLimit != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.Limit = *rateLimitOverrides.ChatRateLimit.RateLimit
	}
	if rateLimitOverrides.ChatRateLimit.RateIntervalSeconds != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.IntervalSeconds = *rateLimitOverrides.ChatRateLimit.RateIntervalSeconds
	}
	if rateLimitOverrides.ChatRateLimit.AllowedModels != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.AllowedModels = rateLimitOverrides.ChatRateLimit.AllowedModels
	}

	return &codyGatewayRateLimitResolver{
		feature:     types.CompletionsFeatureChat,
		actorID:     r.sub.UUID(),
		actorSource: codygateway.ActorSourceProductSubscription,
		v:           rateLimit,
		source:      source,
	}, nil
}

func (r codyGatewayAccessResolver) CodeCompletionsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	if !r.Enabled() {
		return nil, nil
	}

	var rateLimit licensing.CodyGatewayRateLimit

	// Get default access from active license. Call hydrate and access field directly to
	// avoid parsing license key which is done in (*productLicense).Info(), instead just
	// relying on what we know in DB.
	activeLicense, err := r.sub.computeActiveLicense(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "could not get active license")
	}
	var source graphqlbackend.CodyGatewayRateLimitSource
	if activeLicense != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourcePlan
		rateLimit = licensing.NewCodyGatewayCodeRateLimit(licensing.PlanFromTags(activeLicense.LicenseTags), activeLicense.LicenseUserCount, activeLicense.LicenseTags)
	}

	// Apply overrides
	rateLimitOverrides := r.sub.v.CodyGatewayAccess
	if rateLimitOverrides.CodeRateLimit.RateLimit != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.Limit = *rateLimitOverrides.CodeRateLimit.RateLimit
	}
	if rateLimitOverrides.CodeRateLimit.RateIntervalSeconds != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.IntervalSeconds = *rateLimitOverrides.CodeRateLimit.RateIntervalSeconds
	}
	if rateLimitOverrides.CodeRateLimit.AllowedModels != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.AllowedModels = rateLimitOverrides.CodeRateLimit.AllowedModels
	}

	return &codyGatewayRateLimitResolver{
		feature:     types.CompletionsFeatureCode,
		actorID:     r.sub.UUID(),
		actorSource: codygateway.ActorSourceProductSubscription,
		v:           rateLimit,
		source:      source,
	}, nil
}

func (r codyGatewayAccessResolver) EmbeddingsRateLimit(ctx context.Context) (graphqlbackend.CodyGatewayRateLimit, error) {
	if !r.Enabled() {
		return nil, nil
	}

	var rateLimit licensing.CodyGatewayRateLimit

	// Get default access from active license. Call hydrate and access field directly to
	// avoid parsing license key which is done in (*productLicense).Info(), instead just
	// relying on what we know in DB.
	activeLicense, err := r.sub.computeActiveLicense(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "could not get active license")
	}
	var source graphqlbackend.CodyGatewayRateLimitSource
	if activeLicense != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourcePlan
		rateLimit = licensing.NewCodyGatewayEmbeddingsRateLimit(licensing.PlanFromTags(activeLicense.LicenseTags), activeLicense.LicenseUserCount, activeLicense.LicenseTags)
	}

	// Apply overrides
	rateLimitOverrides := r.sub.v.CodyGatewayAccess
	if rateLimitOverrides.EmbeddingsRateLimit.RateLimit != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.Limit = *rateLimitOverrides.EmbeddingsRateLimit.RateLimit
	}
	if rateLimitOverrides.EmbeddingsRateLimit.RateIntervalSeconds != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.IntervalSeconds = *rateLimitOverrides.EmbeddingsRateLimit.RateIntervalSeconds
	}
	if rateLimitOverrides.EmbeddingsRateLimit.AllowedModels != nil {
		source = graphqlbackend.CodyGatewayRateLimitSourceOverride
		rateLimit.AllowedModels = rateLimitOverrides.EmbeddingsRateLimit.AllowedModels
	}

	return &codyGatewayRateLimitResolver{
		actorID:     r.sub.UUID(),
		actorSource: codygateway.ActorSourceProductSubscription,
		v:           rateLimit,
		source:      source,
	}, nil
}

type codyGatewayRateLimitResolver struct {
	actorID     string
	actorSource codygateway.ActorSource
	feature     types.CompletionsFeature
	source      graphqlbackend.CodyGatewayRateLimitSource
	v           licensing.CodyGatewayRateLimit
}

func (r *codyGatewayRateLimitResolver) Source() graphqlbackend.CodyGatewayRateLimitSource {
	return r.source
}

func (r *codyGatewayRateLimitResolver) AllowedModels() []string { return r.v.AllowedModels }

func (r *codyGatewayRateLimitResolver) Limit() int32 { return r.v.Limit }

func (r *codyGatewayRateLimitResolver) IntervalSeconds() int32 { return r.v.IntervalSeconds }

func (r codyGatewayRateLimitResolver) Usage(ctx context.Context) ([]graphqlbackend.CodyGatewayUsageDatapoint, error) {
	var (
		usage []SubscriptionUsage
		err   error
	)
	if r.feature != "" {
		usage, err = NewCodyGatewayService().CompletionsUsageForActor(ctx, r.feature, r.actorSource, r.actorID)
		if err != nil {
			return nil, err
		}
	} else {
		usage, err = NewCodyGatewayService().EmbeddingsUsageForActor(ctx, r.actorSource, r.actorID)
		if err != nil {
			return nil, err
		}
	}

	resolvers := make([]graphqlbackend.CodyGatewayUsageDatapoint, 0, len(usage))
	for _, u := range usage {
		resolvers = append(resolvers, &codyGatewayUsageDatapoint{
			date:  u.Date,
			model: u.Model,
			count: u.Count,
		})
	}

	return resolvers, nil
}

type codyGatewayUsageDatapoint struct {
	date  time.Time
	model string
	count int
}

func (r *codyGatewayUsageDatapoint) Date() gqlutil.DateTime {
	return gqlutil.DateTime{Time: r.date}
}

func (r *codyGatewayUsageDatapoint) Model() string {
	return r.model
}

func (r *codyGatewayUsageDatapoint) Count() int32 {
	return int32(r.count)
}
