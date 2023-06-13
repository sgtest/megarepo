package featurelimiter

import (
	"context"
	"net/http"
	"strings"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/events"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/limiter"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/notify"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/response"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/types"
	sgtrace "github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type contextKey string

const contextKeyFeature contextKey = "feature"

// GetFeature gets the feature used by Handle or HandleFeature.
func GetFeature(ctx context.Context) codygateway.Feature {
	if f, ok := ctx.Value(contextKeyFeature).(codygateway.Feature); ok {
		return f
	}
	return ""
}

// Handle extracts features from codygateway.FeatureHeaderName and uses it to
// determine the appropriate per-feature rate limits applied for an actor.
func Handle(
	baseLogger log.Logger,
	eventLogger events.Logger,
	cache limiter.RedisStore,
	rateLimitNotifier notify.RateLimitNotifier,
	next http.Handler,
) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		feature, err := extractFeature(r)
		if err != nil {
			response.JSONError(baseLogger, w, http.StatusBadRequest, err)
			return
		}

		HandleFeature(baseLogger, eventLogger, cache, rateLimitNotifier, feature, next).
			ServeHTTP(w, r)
	})
}

func extractFeature(r *http.Request) (codygateway.Feature, error) {
	h := strings.TrimSpace(r.Header.Get(codygateway.FeatureHeaderName))
	if h == "" {
		return "", errors.Newf("%s header is required", codygateway.FeatureHeaderName)
	}
	feature := types.CompletionsFeature(h)
	if !feature.IsValid() {
		return "", errors.Newf("invalid value for %s", codygateway.FeatureHeaderName)
	}
	// codygateway.Feature and types.CompletionsFeature map 1:1 for completions.
	return codygateway.Feature(feature), nil
}

// Handle uses a predefined feature to determine the appropriate per-feature
// rate limits applied for an actor.
func HandleFeature(
	baseLogger log.Logger,
	eventLogger events.Logger,
	cache limiter.RedisStore,
	rateLimitNotifier notify.RateLimitNotifier,
	feature codygateway.Feature,
	next http.Handler,
) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		act := actor.FromContext(r.Context())
		logger := act.Logger(sgtrace.Logger(r.Context(), baseLogger))

		r = r.WithContext(context.WithValue(r.Context(), contextKeyFeature, feature))

		l, ok := act.Limiter(logger, cache, feature, rateLimitNotifier)
		if !ok {
			response.JSONError(logger, w, http.StatusForbidden, errors.Newf("no access to feature %s", feature))
			return
		}

		commit, err := l.TryAcquire(r.Context())
		if err != nil {
			limitedCause := "quota"
			defer func() {
				if loggerErr := eventLogger.LogEvent(
					r.Context(),
					events.Event{
						Name:       codygateway.EventNameRateLimited,
						Source:     act.Source.Name(),
						Identifier: act.ID,
						Metadata: map[string]any{
							"error": err.Error(),
							codygateway.CompletionsEventFeatureMetadataField: feature,
							"cause": limitedCause,
						},
					},
				); loggerErr != nil {
					logger.Error("failed to log event", log.Error(loggerErr))
				}
			}()

			var concurrencyLimitExceeded actor.ErrConcurrencyLimitExceeded
			if errors.As(err, &concurrencyLimitExceeded) {
				limitedCause = "concurrency"
				concurrencyLimitExceeded.WriteResponse(w)
				return
			}

			var rateLimitExceeded limiter.RateLimitExceededError
			if errors.As(err, &rateLimitExceeded) {
				rateLimitExceeded.WriteResponse(w)
				return
			}

			if errors.Is(err, limiter.NoAccessError{}) {
				response.JSONError(logger, w, http.StatusForbidden, err)
				return
			}

			response.JSONError(logger, w, http.StatusInternalServerError, err)
			return
		}

		responseRecorder := response.NewStatusHeaderRecorder(w)
		next.ServeHTTP(responseRecorder, r)

		// If response is healthy, consume the rate limit
		if responseRecorder.StatusCode >= 200 && responseRecorder.StatusCode < 300 {
			if err := commit(r.Context(), 1); err != nil {
				logger.Error("failed to commit rate limit consumption", log.Error(err))
			}
		}
	})
}
