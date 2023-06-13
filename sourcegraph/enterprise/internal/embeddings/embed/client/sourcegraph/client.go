package sourcegraph

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/dotcomuser"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/embed/client"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

const defaultModel = "openai/text-embedding-ada-002"

func NewClient(config *schema.SiteConfiguration) *sourcegraphEmbeddingsClient {
	return &sourcegraphEmbeddingsClient{
		model:       getModel(config),
		dimensions:  config.Embeddings.Dimensions,
		url:         getURL(config.Embeddings),
		accessToken: getAccessToken(config),
	}
}

const defaultAPIURL = "https://cody-gateway.sourcegraph.com/v1/embeddings"

func getModel(config *schema.SiteConfiguration) string {
	if config.Embeddings == nil || config.Embeddings.Model == "" {
		return defaultModel
	}
	return strings.ToLower(config.Embeddings.Model)
}

func getAccessToken(config *schema.SiteConfiguration) string {
	// If an access token is configured, use it.
	if config.Embeddings.AccessToken != "" {
		return config.Embeddings.AccessToken
	}
	// App generates a token from the api token the user used to connect app to dotcom.
	if deploy.IsApp() && config.App != nil {
		return dotcomuser.GenerateDotcomUserGatewayAccessToken(config.App.DotcomAuthToken)
	}
	// Otherwise, use the current license key to compute an access token.
	return licensing.GenerateLicenseKeyBasedAccessToken(config.LicenseKey)
}

func getURL(config *schema.Embeddings) string {
	url := config.Endpoint
	// Fallback to URL, it's the previous name of the setting.
	if url == "" {
		url = config.Url
	}
	// If that is also not set, use a sensible default.
	if url == "" {
		url = defaultAPIURL
	}
	return url
}

type sourcegraphEmbeddingsClient struct {
	model       string
	dimensions  int
	url         string
	accessToken string
}

func (c *sourcegraphEmbeddingsClient) GetDimensions() (int, error) {
	if c.dimensions <= 0 && strings.EqualFold(c.model, defaultModel) {
		return 1536, nil
	}

	// TODO: Later, we should ideally ask the gateway for the dimensionality of the model
	// so we don't have to hard-code defaults for all the models and can roll out new models
	// to older instances, too.
	if c.dimensions <= 0 {
		return 0, errors.New("invalid config for embeddings.dimensions, must be > 0")
	}

	return c.dimensions, nil
}

func (c *sourcegraphEmbeddingsClient) GetModelIdentifier() string {
	// Special-case the default model, since it already includes the provider name
	if strings.EqualFold(c.model, defaultModel) {
		return defaultModel
	}
	return fmt.Sprintf("sourcegraph/%s", c.model)
}

// GetEmbeddingsWithRetries tries to embed the given texts using the external service specified in the config.
// In case of failure, it retries the embedding procedure up to maxRetries. This due to the OpenAI API which
// often hangs up when downloading large embedding responses.
func (c *sourcegraphEmbeddingsClient) GetEmbeddingsWithRetries(ctx context.Context, texts []string, maxRetries int) ([]float32, error) {
	embeddings, err := c.getEmbeddings(ctx, texts)
	if err == nil {
		return embeddings, nil
	}

	for i := 0; i < maxRetries; i++ {
		embeddings, err = c.getEmbeddings(ctx, texts)
		if err == nil {
			return embeddings, nil
		} else {
			// Exponential delay
			delay := time.Duration(int(math.Pow(float64(2), float64(i))))
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(delay * time.Second):
			}
		}
	}

	return nil, err
}

var modelsWithoutNewlines = map[string]struct{}{
	"openai/text-embedding-ada-002": {},
}

func (c *sourcegraphEmbeddingsClient) getEmbeddings(ctx context.Context, texts []string) ([]float32, error) {
	_, replaceNewlines := modelsWithoutNewlines[c.model]
	augmentedTexts := texts
	if replaceNewlines {
		augmentedTexts = make([]string, len(texts))
		// Replace newlines for certain (OpenAI) models, because they can negatively affect performance.
		for idx, text := range texts {
			augmentedTexts[idx] = strings.ReplaceAll(text, "\n", " ")
		}
	}

	request := codygateway.EmbeddingsRequest{Model: c.model, Input: augmentedTexts}

	bodyBytes, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.url, bytes.NewReader(bodyBytes))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.accessToken)
	if len(texts) > 1 {
		req.Header.Set("X-Cody-Embed-Batch-Size", strconv.Itoa(len(texts)))
	}
	resp, err := httpcli.ExternalDoer.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		if resp.StatusCode == http.StatusTooManyRequests {
			retryAfterHeader := resp.Header.Get("retry-after")
			if retryAfterHeader != "" {
				// There are two valid formats for retry-after headers: seconds
				// until retry in int, or a RFC1123 date string.
				// First, see if it is denoted in seconds.
				s, err := strconv.Atoi(retryAfterHeader)
				// If denoted in seconds, only retry if we will get access within
				// the next retryAfterMaxSleepDuration seconds.
				if err == nil {
					return nil, client.NewRateLimitExceededError(time.Now().Add(time.Duration(s) * time.Second))
				}

				// If we weren't able to parse as seconds, try to parse as RFC1123.
				after, err := time.Parse(time.RFC1123, retryAfterHeader)
				if err == nil {
					return nil, client.NewRateLimitExceededError(after)
				}
				// We don't know how to parse this header, so let's just return a generic error.
			}
		}
		respBody, _ := io.ReadAll(io.LimitReader(resp.Body, 1024))
		return nil, errors.Errorf("embeddings: %s %q: failed with status %d: %s", req.Method, req.URL.String(), resp.StatusCode, string(respBody))
	}

	var response codygateway.EmbeddingsResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return nil, err
	}

	if len(response.Embeddings) == 0 {
		return nil, nil
	}

	// Ensure embedding responses are sorted in the original order.
	sort.Slice(response.Embeddings, func(i, j int) bool {
		return response.Embeddings[i].Index < response.Embeddings[j].Index
	})

	embeddings := make([]float32, 0, len(response.Embeddings)*response.ModelDimensions)
	for _, embedding := range response.Embeddings {
		embeddings = append(embeddings, embedding.Data...)
	}

	return embeddings, nil
}
