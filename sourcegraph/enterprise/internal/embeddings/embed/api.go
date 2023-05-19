package embed

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"math"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

type EmbeddingAPIRequest struct {
	Model string   `json:"model"`
	Input []string `json:"input"`
}

type EmbeddingAPIResponse struct {
	Data []struct {
		Index     int       `json:"index"`
		Embedding []float32 `json:"embedding"`
	} `json:"data"`
}

type EmbeddingsClient interface {
	GetEmbeddingsWithRetries(ctx context.Context, texts []string, maxRetries int) ([]float32, error)
	GetDimensions() (int, error)
}

func NewEmbeddingsClient() EmbeddingsClient {
	return &embeddingsClient{conf.Get().Embeddings}
}

type embeddingsClient struct {
	config *schema.Embeddings
}

// isDisabled checks the current state of the site config to see if embeddings are
// enabled. This gives an "escape hatch" for cancelling a long-running embeddings job.
func (c *embeddingsClient) isDisabled() bool {
	return !conf.EmbeddingsEnabled()
}

func (c *embeddingsClient) GetDimensions() (int, error) {
	if c.isDisabled() {
		return -1, errors.New("embeddings are not configured or disabled")
	}
	return c.config.Dimensions, nil
}

// GetEmbeddingsWithRetries tries to embed the given texts using the external service specified in the config.
// In case of failure, it retries the embedding procedure up to maxRetries. This due to the OpenAI API which
// often hangs up when downloading large embedding responses.
func (c *embeddingsClient) GetEmbeddingsWithRetries(ctx context.Context, texts []string, maxRetries int) ([]float32, error) {
	if c.isDisabled() {
		return nil, errors.New("embeddings are not configured or disabled")
	}

	embeddings, err := GetEmbeddings(ctx, texts, c.config)
	if err == nil {
		return embeddings, nil
	}

	for i := 0; i < maxRetries; i++ {
		embeddings, err = GetEmbeddings(ctx, texts, c.config)
		if err == nil {
			return embeddings, nil
		} else {
			// Exponential delay
			delay := time.Duration(int(math.Pow(float64(2), float64(i))))
			time.Sleep(delay * time.Second)
		}
	}

	return nil, err
}

var MODELS_WITHOUT_NEWLINES = map[string]struct{}{
	"text-embedding-ada-002": {},
}

func GetEmbeddings(ctx context.Context, texts []string, config *schema.Embeddings) ([]float32, error) {
	_, replaceNewlines := MODELS_WITHOUT_NEWLINES[config.Model]
	augmentedTexts := texts
	if replaceNewlines {
		augmentedTexts = make([]string, len(texts))
		// Replace newlines for certain (OpenAI) models, because they can negatively affect performance.
		for idx, text := range texts {
			augmentedTexts[idx] = strings.ReplaceAll(text, "\n", " ")
		}
	}

	request := EmbeddingAPIRequest{Model: config.Model, Input: augmentedTexts}

	bodyBytes, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, "POST", config.Url, bytes.NewReader(bodyBytes))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+config.AccessToken)

	resp, err := httpcli.ExternalDoer.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		respBody, _ := io.ReadAll(resp.Body)
		return nil, errors.Errorf("embeddings: %s %q: failed with status %d: %s", req.Method, req.URL.String(), resp.StatusCode, string(respBody))
	}

	var response EmbeddingAPIResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return nil, err
	}

	// Ensure embedding responses are sorted in the original order.
	sort.Slice(response.Data, func(i, j int) bool {
		return response.Data[i].Index < response.Data[j].Index
	})

	embeddings := make([]float32, 0, len(response.Data)*config.Dimensions)
	for _, embedding := range response.Data {
		embeddings = append(embeddings, embedding.Embedding...)
	}
	return embeddings, nil
}
