package resolvers

import (
	"context"
	"strings"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/cody"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/client"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/httpapi"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/redispool"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var _ graphqlbackend.CompletionsResolver = &completionsResolver{}

// completionsResolver provides chat completions
type completionsResolver struct {
	rl     httpapi.RateLimiter
	db     database.DB
	logger log.Logger
}

func NewCompletionsResolver(db database.DB, logger log.Logger) graphqlbackend.CompletionsResolver {
	rl := httpapi.NewRateLimiter(db, redispool.Store, types.CompletionsFeatureChat)
	return &completionsResolver{rl: rl, db: db, logger: logger}
}

func (c *completionsResolver) Completions(ctx context.Context, args graphqlbackend.CompletionsArgs) (_ string, err error) {
	if isEnabled := cody.IsCodyEnabled(ctx); !isEnabled {
		return "", errors.New("cody experimental feature flag is not enabled for current user")
	}

	if err := cody.CheckVerifiedEmailRequirement(ctx, c.db, c.logger); err != nil {
		return "", err
	}

	completionsConfig := client.GetCompletionsConfig(conf.Get().SiteConfig())
	if completionsConfig == nil || !completionsConfig.Enabled {
		return "", errors.New("completions are not configured or disabled")
	}

	ctx, done := httpapi.Trace(ctx, "resolver", completionsConfig.ChatModel).
		WithErrorP(&err).
		Build()
	defer done()

	client, err := client.Get(
		completionsConfig.Endpoint,
		completionsConfig.Provider,
		completionsConfig.AccessToken,
	)
	if err != nil {
		return "", errors.Wrap(err, "GetCompletionStreamClient")
	}

	// Check rate limit.
	if err := c.rl.TryAcquire(ctx); err != nil {
		return "", err
	}

	params := convertParams(args)
	// No way to configure the model through the request, we hard code to chat.
	params.Model = completionsConfig.ChatModel
	resp, err := client.Complete(ctx, types.CompletionsFeatureChat, params)
	if err != nil {
		return "", errors.Wrap(err, "client.Complete")
	}
	return resp.Completion, nil
}

func convertParams(args graphqlbackend.CompletionsArgs) types.CompletionRequestParameters {
	return types.CompletionRequestParameters{
		Messages:          convertMessages(args.Input.Messages),
		Temperature:       float32(args.Input.Temperature),
		MaxTokensToSample: int(args.Input.MaxTokensToSample),
		TopK:              int(args.Input.TopK),
		TopP:              float32(args.Input.TopP),
	}
}

func convertMessages(messages []graphqlbackend.Message) (result []types.Message) {
	for _, message := range messages {
		result = append(result, types.Message{
			Speaker: strings.ToLower(message.Speaker),
			Text:    message.Text,
		})
	}
	return result
}
