package anonymous

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/httpapi/embeddings"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
)

type Source struct {
	allowAnonymous bool
}

func NewSource(allowAnonymous bool) *Source {
	return &Source{allowAnonymous: allowAnonymous}
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
			codygateway.FeatureChatCompletions: {
				AllowedModels: []string{"anthropic/claude-v1"},
				Limit:         50,
				Interval:      24 * time.Hour,
			},
			codygateway.FeatureCodeCompletions: {
				AllowedModels: []string{"anthropic/claude-instant-v1"},
				Limit:         500,
				Interval:      24 * time.Hour,
			},
			codygateway.FeatureEmbeddings: {
				AllowedModels: []string{string(embeddings.ModelNameOpenAIAda)},
				Limit:         100_000,
				Interval:      24 * time.Hour,
			},
		},
		Source: s,
	}, nil
}
