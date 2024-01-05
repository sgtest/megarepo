package completions

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/completions/client/fireworks"

	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/events"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/limiter"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/notify"
	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
)

const fireworksAPIURL = "https://api.fireworks.ai/inference/v1/completions"
const fireworksChatAPIURL = "https://api.fireworks.ai/inference/v1/chat/completions"

func NewFireworksHandler(
	baseLogger log.Logger,
	eventLogger events.Logger,
	rs limiter.RedisStore,
	rateLimitNotifier notify.RateLimitNotifier,
	httpClient httpcli.Doer,
	accessToken string,
	allowedModels []string,
	logSelfServeCodeCompletionRequests bool,
	disableSingleTenant bool,
	autoFlushStreamingResponses bool,
) http.Handler {
	return makeUpstreamHandler[fireworksRequest](
		baseLogger,
		eventLogger,
		rs,
		rateLimitNotifier,
		httpClient,
		string(conftypes.CompletionsProviderNameFireworks),
		func(feature codygateway.Feature) string {
			if feature == codygateway.FeatureChatCompletions {
				return fireworksChatAPIURL
			} else {
				return fireworksAPIURL
			}
		},
		allowedModels,
		&FireworksHandlerMethods{accessToken: accessToken, baseLogger: baseLogger, eventLogger: eventLogger, logSelfServeCodeCompletionRequests: logSelfServeCodeCompletionRequests, disableSingleTenant: disableSingleTenant},

		// Setting to a valuer higher than SRC_HTTP_CLI_EXTERNAL_RETRY_AFTER_MAX_DURATION to not
		// do any retries
		30, // seconds
		autoFlushStreamingResponses,
	)
}

// fireworksRequest captures fields from https://readme.fireworks.ai/reference/createcompletion and
// https://readme.fireworks.ai/reference/createchatcompletion.
type fireworksRequest struct {
	Prompt      string    `json:"prompt,omitempty"`
	Messages    []message `json:"messages,omitempty"`
	Model       string    `json:"model"`
	MaxTokens   int32     `json:"max_tokens,omitempty"`
	Temperature float32   `json:"temperature,omitempty"`
	TopP        float32   `json:"top_p,omitempty"`
	N           int32     `json:"n,omitempty"`
	Stream      bool      `json:"stream,omitempty"`
	Echo        bool      `json:"echo,omitempty"`
	Stop        []string  `json:"stop,omitempty"`
}

func (fr fireworksRequest) ShouldStream() bool {
	return fr.Stream
}

func (fr fireworksRequest) GetModel() string {
	return fr.Model
}

type message struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type fireworksResponse struct {
	Choices []struct {
		Text         string `json:"text"`
		Index        int    `json:"index"`
		FinishReason string `json:"finish_reason"`
	} `json:"choices"`
	Usage struct {
		PromptTokens     int `json:"prompt_tokens"`
		TotalTokens      int `json:"total_tokens"`
		CompletionTokens int `json:"completion_tokens"`
	} `json:"usage"`
}

type FireworksHandlerMethods struct {
	accessToken                        string
	logSelfServeCodeCompletionRequests bool
	disableSingleTenant                bool
	baseLogger                         log.Logger
	eventLogger                        events.Logger
}

func (f *FireworksHandlerMethods) validateRequest(_ context.Context, _ log.Logger, _ codygateway.Feature, _ fireworksRequest) (int, *flaggingResult, error) {
	return 0, nil, nil
}
func (f *FireworksHandlerMethods) transformBody(body *fireworksRequest, _ string) {
	// We don't want to let users generate multiple responses, as this would
	// mess with rate limit counting.
	if body.N > 1 {
		body.N = 1
	}
	if f.disableSingleTenant {
		oldModel := body.Model
		if body.Model == "accounts/sourcegraph/models/starcoder-16b" {
			body.Model = "accounts/fireworks/models/starcoder-16b-w8a16"
		} else if body.Model == "accounts/sourcegraph/models/starcoder-7b" {
			body.Model = "accounts/fireworks/models/starcoder-7b-w8a16"
		}
		if oldModel != body.Model {
			f.baseLogger.Debug("rewriting model", log.String("old-model", oldModel), log.String("new-model", body.Model))
		}
	}
}
func (f *FireworksHandlerMethods) getRequestMetadata(ctx context.Context, logger log.Logger, act *actor.Actor, feature codygateway.Feature, body fireworksRequest) (model string, additionalMetadata map[string]any) {
	// Check that this is a code completion request and that the actor is a PLG user
	if feature == codygateway.FeatureCodeCompletions && f.logSelfServeCodeCompletionRequests && act.IsDotComActor() {
		// LogEvent is a channel send (not an external request), so should be ok here
		if err := f.eventLogger.LogEvent(
			ctx,
			events.Event{
				Name:       codygateway.EventNameCodeCompletionLogged,
				Source:     act.Source.Name(),
				Identifier: act.ID,
				Metadata: map[string]any{
					"request": map[string]any{
						"prompt":      body.Prompt,
						"model":       body.Model,
						"max_tokens":  body.MaxTokens,
						"temperature": body.Temperature,
						"top_p":       body.TopP,
						"n":           body.N,
						"stream":      body.Stream,
						"echo":        body.Echo,
						"stop":        body.Stop,
					},
				},
			}); err != nil {
			logger.Error("failed to log event", log.Error(err))
		}
	}
	return body.Model, map[string]any{"stream": body.Stream}
}
func (f *FireworksHandlerMethods) transformRequest(r *http.Request) {
	r.Header.Set("Content-Type", "application/json")
	r.Header.Set("Authorization", "Bearer "+f.accessToken)
}
func (f *FireworksHandlerMethods) parseResponseAndUsage(logger log.Logger, reqBody fireworksRequest, r io.Reader) (promptUsage, completionUsage usageStats) {
	// First, extract prompt usage details from the request.
	promptUsage.characters = len(reqBody.Prompt)

	// Try to parse the request we saw, if it was non-streaming, we can simply parse
	// it as JSON.
	if !reqBody.Stream {
		var res fireworksResponse
		if err := json.NewDecoder(r).Decode(&res); err != nil {
			logger.Error("failed to parse fireworks response as JSON", log.Error(err))
			return promptUsage, completionUsage
		}

		promptUsage.tokens = res.Usage.PromptTokens
		completionUsage.tokens = res.Usage.CompletionTokens
		if len(res.Choices) > 0 {
			// TODO: Later, we should look at the usage field.
			completionUsage.characters = len(res.Choices[0].Text)
		}
		return promptUsage, completionUsage
	}

	// Otherwise, we have to parse the event stream.
	//
	// TODO: Does fireworks streaming include usage data?
	// Unclear in the API currently: https://readme.fireworks.ai/reference/createcompletion
	// For now, just count character usage, and set token counts to
	// -1 as sentinel values.
	promptUsage.tokens = -1
	completionUsage.tokens = -1

	dec := fireworks.NewDecoder(r)
	// Consume all the messages, but we only care about the last completion data.
	for dec.Scan() {
		data := dec.Data()

		// Gracefully skip over any data that isn't JSON-like.
		if !bytes.HasPrefix(data, []byte("{")) {
			continue
		}

		var event fireworksResponse
		if err := json.Unmarshal(data, &event); err != nil {
			logger.Error("failed to decode event payload", log.Error(err), log.String("body", string(data)))
			continue
		}

		if len(event.Choices) > 0 {
			completionUsage.characters += len(event.Choices[0].Text)
		}
	}
	if err := dec.Err(); err != nil {
		logger.Error("failed to decode Fireworks streaming response", log.Error(err))
	}

	return promptUsage, completionUsage
}
