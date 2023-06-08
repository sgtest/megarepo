package anonymous

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/httpapi/embeddings"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
)

type Source struct {
	allowAnonymous    bool
	concurrencyConfig codygateway.ActorConcurrencyLimitConfig
}

func NewSource(allowAnonymous bool, concurrencyConfig codygateway.ActorConcurrencyLimitConfig) *Source {
	return &Source{allowAnonymous: allowAnonymous, concurrencyConfig: concurrencyConfig}
}

var _ actor.Source = &Source{}

func (s *Source) Name() string { return "anonymous" }

func (s *Source) Get(ctx context.Context, token string) (*actor.Actor, error) {
	// This source only handles completely anonymous requests.
	if token != "" {
		return nil, actor.ErrNotFromSource{}
	}
	return &actor.Actor{
		ID:            "anonymous", // TODO: Make this IP-based?
		Key:           token,
		AccessEnabled: s.allowAnonymous,
		// Some basic defaults for chat and code completions.
		RateLimits: map[codygateway.Feature]actor.RateLimit{
			codygateway.FeatureChatCompletions: actor.NewRateLimitWithPercentageConcurrency(
				50,
				24*time.Hour,
				[]string{"anthropic/claude-v1"},
				s.concurrencyConfig,
			),
			codygateway.FeatureCodeCompletions: actor.NewRateLimitWithPercentageConcurrency(
				1000,
				24*time.Hour,
				[]string{"anthropic/claude-instant-v1"},
				s.concurrencyConfig,
			),
			codygateway.FeatureEmbeddings: {
				AllowedModels: []string{string(embeddings.ModelNameOpenAIAda)},
				Limit:         100_000,
				Interval:      24 * time.Hour,

				// Allow 10 concurrent requests for now for anonymous users.
				ConcurrentRequests:         10,
				ConcurrentRequestsInterval: s.concurrencyConfig.Interval,
			},
		},
		Source: s,
	}, nil
}
