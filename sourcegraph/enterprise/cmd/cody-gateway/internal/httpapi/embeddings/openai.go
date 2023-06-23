package embeddings

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"sort"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/response"
	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func NewOpenAIClient(httpClient *http.Client, accessToken string) EmbeddingsClient {
	return &openaiClient{
		httpClient:  httpClient,
		accessToken: accessToken,
	}
}

type openaiClient struct {
	httpClient  *http.Client
	accessToken string
}

const apiURL = "https://api.openai.com/v1/embeddings"

func (c *openaiClient) GenerateEmbeddings(ctx context.Context, input codygateway.EmbeddingsRequest) (*codygateway.EmbeddingsResponse, int, error) {
	for _, s := range input.Input {
		if s == "" {
			// The OpenAI API will return an error if any of the strings in texts is an empty string,
			// so fail fast to avoid making tons of retryable requests.
			return nil, 0, response.NewHTTPStatusCodeError(http.StatusBadRequest, errors.New("cannot generate embeddings for an empty string"))
		}
	}

	model, ok := openAIModelMappings[input.Model]
	if !ok {
		return nil, 0, response.NewHTTPStatusCodeError(http.StatusBadRequest, errors.Newf("no OpenAI model found for %q", input.Model))
	}

	response, err := c.requestEmbeddings(ctx, model, input.Input)
	if err != nil {
		return nil, 0, err
	}
	// Ensure embedding responses are sorted in the original order.
	sort.Slice(response.Data, func(i, j int) bool {
		return response.Data[i].Index < response.Data[j].Index
	})

	embeddings := make([]codygateway.Embedding, len(response.Data))
	for i, d := range response.Data {
		if len(d.Embedding) > 0 {
			embeddings[i] = codygateway.Embedding{
				Index: d.Index,
				Data:  d.Embedding,
			}
		} else {
			// HACK(camdencheek): Nondeterministically, the OpenAI API will
			// occasionally send back a `null` for an embedding in the
			// response. Try it again a few times and hope for the best.
			resp, err := c.requestSingleEmbeddingWithRetryOnNull(ctx, model, input.Input[d.Index], 3)
			if err != nil {
				return nil, 0, err
			}
			embeddings[i] = codygateway.Embedding{
				Index: d.Index,
				Data:  resp.Data[0].Embedding,
			}
		}
	}

	return &codygateway.EmbeddingsResponse{
		Embeddings:      embeddings,
		Model:           response.Model,
		ModelDimensions: model.dimensions,
	}, response.Usage.TotalTokens, nil
}

func (c *openaiClient) requestSingleEmbeddingWithRetryOnNull(ctx context.Context, model openAIModel, input string, retries int) (resp *openaiEmbeddingsResponse, err error) {
	for i := 0; i < retries; i++ {
		resp, err = c.requestEmbeddings(ctx, model, []string{input})
		if err != nil {
			return nil, err
		}
		if len(resp.Data) != 1 || len(resp.Data[0].Embedding) != model.dimensions {
			continue // retry
		}
		return resp, err
	}
	return nil, errors.Newf("null response for embedding after %d retries", retries)
}

func (c *openaiClient) requestEmbeddings(ctx context.Context, model openAIModel, input []string) (*openaiEmbeddingsResponse, error) {
	request := openaiEmbeddingsRequest{
		Model: model.upstreamName,
		Input: input,
		// TODO: Maybe set user.
	}

	bodyBytes, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, apiURL, bytes.NewReader(bodyBytes))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", "Bearer "+c.accessToken)

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 300 || resp.StatusCode < 200 {
		// If we are being rate limited by OpenAI, we don't want to forward that error and instead
		// return a 503 to the client. It's not them being limited, it's us and that an operations
		// error on our side.
		if resp.StatusCode == http.StatusTooManyRequests {
			return nil, response.NewHTTPStatusCodeError(http.StatusServiceUnavailable, errors.Newf("we're facing too much load at the moment, please retry"))
		}
		respBody, _ := io.ReadAll(io.LimitReader(resp.Body, 1024))
		// We don't forward the status code here, everything but 429 should turn into a 500 error.
		return nil, errors.Errorf("embeddings: %s %q: failed with status %d: %s", req.Method, req.URL.String(), resp.StatusCode, string(respBody))
	}

	var response openaiEmbeddingsResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		// Although we might've incurred cost at this point, we don't want to count
		// that towards the rate limit of the requester, so return 0 for the consumed
		// token count.
		return nil, err
	}

	return &response, nil
}

type openaiEmbeddingsRequest struct {
	Model string   `json:"model"`
	Input []string `json:"input"`
	User  string   `json:"user"`
}

type openaiEmbeddingsUsage struct {
	PromptTokens int `json:"prompt_tokens"`
	TotalTokens  int `json:"total_tokens"`
}

type openaiEmbeddingsData struct {
	Index     int       `json:"index"`
	Embedding []float32 `json:"embedding"`
}

type openaiEmbeddingsResponse struct {
	Model string                 `json:"model"`
	Usage openaiEmbeddingsUsage  `json:"usage"`
	Data  []openaiEmbeddingsData `json:"data"`
}

type openAIModel struct {
	upstreamName string
	dimensions   int
}

var openAIModelMappings = map[string]openAIModel{
	string(ModelNameOpenAIAda): {
		upstreamName: "text-embedding-ada-002",
		dimensions:   1536,
	},
}
