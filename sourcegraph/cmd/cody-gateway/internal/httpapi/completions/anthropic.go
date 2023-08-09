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
	"github.com/sourcegraph/sourcegraph/internal/completions/client/anthropic"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const anthropicAPIURL = "https://api.anthropic.com/v1/complete"

func NewAnthropicHandler(
	logger log.Logger,
	eventLogger events.Logger,
	rs limiter.RedisStore,
	rateLimitNotifier notify.RateLimitNotifier,
	httpClient httpcli.Doer,
	accessToken string,
	allowedModels []string,
	maxTokensToSample int,
) http.Handler {
	return makeUpstreamHandler(
		logger,
		eventLogger,
		rs,
		rateLimitNotifier,
		httpClient,
		string(conftypes.CompletionsProviderNameAnthropic),
		anthropicAPIURL,
		allowedModels,
		upstreamHandlerMethods[anthropicRequest]{
			validateRequest: func(_ codygateway.Feature, ar anthropicRequest) (int, error) {
				if ar.MaxTokensToSample > int32(maxTokensToSample) {
					return http.StatusBadRequest, errors.Errorf("max_tokens_to_sample exceeds maximum allowed value of %d: %d", maxTokensToSample, ar.MaxTokensToSample)
				}
				return 0, nil
			},
			transformBody: func(body *anthropicRequest, act *actor.Actor) {
				// Overwrite the metadata field, we don't want to allow users to specify it:
				body.Metadata = &anthropicRequestMetadata{
					// We forward the actor ID to support tracking.
					UserID: act.ID,
				}
			},
			getRequestMetadata: func(body anthropicRequest) (promptCharacterCount int, model string, additionalMetadata map[string]any) {
				return len(body.Prompt), body.Model, map[string]any{"stream": body.Stream}
			},
			transformRequest: func(r *http.Request) {
				// Mimic headers set by the official Anthropic client:
				// https://sourcegraph.com/github.com/anthropics/anthropic-sdk-typescript@493075d70f50f1568a276ed0cb177e297f5fef9f/-/blob/src/index.ts
				r.Header.Set("Cache-Control", "no-cache")
				r.Header.Set("Accept", "application/json")
				r.Header.Set("Content-Type", "application/json")
				r.Header.Set("Client", "sourcegraph-cody-gateway/1.0")
				r.Header.Set("X-API-Key", accessToken)
				r.Header.Set("anthropic-version", "2023-01-01")
			},
			parseResponse: func(reqBody anthropicRequest, r io.Reader) int {
				// Try to parse the request we saw, if it was non-streaming, we can simply parse
				// it as JSON.
				if !reqBody.Stream {
					var res anthropicResponse
					if err := json.NewDecoder(r).Decode(&res); err != nil {
						logger.Error("failed to parse anthropic response as JSON", log.Error(err))
						return 0
					}
					return len(res.Completion)
				}

				// Otherwise, we have to parse the event stream from anthropic.
				dec := anthropic.NewDecoder(r)
				var lastCompletion string
				// Consume all the messages, but we only care about the last completion data.
				for dec.Scan() {
					data := dec.Data()

					// Gracefully skip over any data that isn't JSON-like. Anthropic's API sometimes sends
					// non-documented data over the stream, like timestamps.
					if !bytes.HasPrefix(data, []byte("{")) {
						continue
					}

					var event anthropicResponse
					if err := json.Unmarshal(data, &event); err != nil {
						logger.Error("failed to decode event payload", log.Error(err), log.String("body", string(data)))
						continue
					}
					lastCompletion = event.Completion
				}

				if err := dec.Err(); err != nil {
					logger.Error("failed to decode Anthropic streaming response", log.Error(err))
				}
				return len(lastCompletion)
			},
		},

		// Anthropic primarily uses concurrent requests to rate-limit spikes
		// in requests, so set a default retry-after that is likely to be
		// acceptable for Sourcegraph clients to retry (the default
		// SRC_HTTP_CLI_EXTERNAL_RETRY_AFTER_MAX_DURATION) since we might be
		// able to circumvent concurrents limits without raising an error to the
		// user.
		2, // seconds
	)
}

// anthropicRequest captures all known fields from https://console.anthropic.com/docs/api/reference.
type anthropicRequest struct {
	Prompt            string                    `json:"prompt"`
	Model             string                    `json:"model"`
	MaxTokensToSample int32                     `json:"max_tokens_to_sample"`
	StopSequences     []string                  `json:"stop_sequences,omitempty"`
	Stream            bool                      `json:"stream,omitempty"`
	Temperature       float32                   `json:"temperature,omitempty"`
	TopK              int32                     `json:"top_k,omitempty"`
	TopP              int32                     `json:"top_p,omitempty"`
	Metadata          *anthropicRequestMetadata `json:"metadata,omitempty"`
}

type anthropicRequestMetadata struct {
	UserID string `json:"user_id,omitempty"`
}

// anthropicResponse captures all relevant-to-us fields from https://console.anthropic.com/docs/api/reference.
type anthropicResponse struct {
	Completion string `json:"completion,omitempty"`
	StopReason string `json:"stop_reason,omitempty"`
}
