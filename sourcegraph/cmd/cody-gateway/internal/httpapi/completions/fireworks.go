package completions

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/events"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/limiter"
	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/notify"
	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/internal/completions/client/fireworks"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const fireworksAPIURL = "https://api.fireworks.ai/inference/v1/completions"

func NewFireworksHandler(
	logger log.Logger,
	eventLogger events.Logger,
	rs limiter.RedisStore,
	rateLimitNotifier notify.RateLimitNotifier,
	httpClient httpcli.Doer,
	accessToken string,
	allowedModels []string,
) http.Handler {
	return makeUpstreamHandler(
		logger,
		eventLogger,
		rs,
		rateLimitNotifier,
		httpClient,
		string(conftypes.CompletionsProviderNameFireworks),
		fireworksAPIURL,
		allowedModels,
		upstreamHandlerMethods[fireworksRequest]{
			validateRequest: func(feature codygateway.Feature, fr fireworksRequest) (int, error) {
				if feature != codygateway.FeatureCodeCompletions {
					return http.StatusNotImplemented,
						errors.Newf("feature %q is currently not supported for Fireworks",
							feature)
				}
				return 0, nil
			},
			transformBody: func(body *fireworksRequest, act *actor.Actor) {
				// We don't want to let users generate multiple responses, as this would
				// mess with rate limit counting.
				if body.N > 1 {
					body.N = 1
				}
			},
			getRequestMetadata: func(body fireworksRequest) (promptCharacterCount int, model string, additionalMetadata map[string]any) {
				return len(body.Prompt), body.Model, map[string]any{"stream": body.Stream}
			},
			transformRequest: func(r *http.Request) {
				r.Header.Set("Content-Type", "application/json")
				r.Header.Set("Authorization", "Bearer "+accessToken)
			},
			parseResponse: func(reqBody fireworksRequest, r io.Reader) int {
				// Try to parse the request we saw, if it was non-streaming, we can simply parse
				// it as JSON.
				if !reqBody.Stream {
					var res fireworksResponse
					if err := json.NewDecoder(r).Decode(&res); err != nil {
						logger.Error("failed to parse fireworks response as JSON", log.Error(err))
						return 0
					}
					if len(res.Choices) > 0 {
						// TODO: Later, we should look at the usage field.
						return len(res.Choices[0].Text)
					}
					return 0
				}

				// Otherwise, we have to parse the event stream.
				dec := fireworks.NewDecoder(r)
				var finalCompletion string
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
						finalCompletion += event.Choices[0].Text
					}
				}

				if err := dec.Err(); err != nil {
					logger.Error("failed to decode Fireworks streaming response", log.Error(err))
				}
				return len(finalCompletion)
			},
		},

		// Setting to a valuer higher than SRC_HTTP_CLI_EXTERNAL_RETRY_AFTER_MAX_DURATION to not
		// do any retries
		30, // seconds
	)
}

// fireworksRequest captures all known fields from https://fireworksai.readme.io/reference/createcompletion.
type fireworksRequest struct {
	Prompt      string   `json:"prompt"`
	Model       string   `json:"model"`
	MaxTokens   int32    `json:"max_tokens,omitempty"`
	Temperature float32  `json:"temperature,omitempty"`
	TopP        int32    `json:"top_p,omitempty"`
	N           int32    `json:"n,omitempty"`
	Stream      bool     `json:"stream,omitempty"`
	Echo        bool     `json:"echo,omitempty"`
	Stop        []string `json:"stop,omitempty"`
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
