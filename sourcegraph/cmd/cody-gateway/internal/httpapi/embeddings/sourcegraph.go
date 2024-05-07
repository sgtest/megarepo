package embeddings

import (
	"bytes"
	"context"
	"io"
	"net/http"

	"github.com/google/uuid"
	jsoniter "github.com/json-iterator/go"
	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/sourcegraph/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
)

const (
	modelDimensions           = 768
	inferenceSecretHeaderName = "sourcegraph-smega-inference-auth"
)

func NewSourcegraphClient(httpClient httpcli.Doer, apiURL string, apiToken string) EmbeddingsClient {
	return &sourcegraphClient{
		httpClient: httpClient,
		json:       jsoniter.ConfigCompatibleWithStandardLibrary,
		apiURL:     apiURL,
		apiToken:   apiToken,
	}
}

type sourcegraphClient struct {
	httpClient httpcli.Doer
	json       jsoniter.API
	apiURL     string
	apiToken   string
}

func (s sourcegraphClient) ProviderName() string {
	return "Sourcegraph"
}

// GenerateEmbeddings uses a KServe-compatible API to generate embedding vectors for items from request.Input
func (s sourcegraphClient) GenerateEmbeddings(ctx context.Context, request codygateway.EmbeddingsRequest) (*codygateway.EmbeddingsResponse, int, error) {
	items := len(request.Input)
	input := kserveInput{
		Name:     "TEXT",
		Shape:    []int{items},
		Datatype: "BYTES",
		Data:     request.Input,
	}
	smegaRequest := kserveRequest{
		ID:     uuid.New().String(),
		Inputs: []kserveInput{input},
	}
	dat, err := s.json.Marshal(smegaRequest)
	if err != nil {
		return nil, 0, err
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, s.apiURL, bytes.NewReader(dat))
	if err != nil {
		return nil, 0, err
	}
	req.Header.Set(inferenceSecretHeaderName, s.apiToken)
	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, 0, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		respBody, err := io.ReadAll(io.LimitReader(resp.Body, 1024))
		if err != nil {
			return nil, 0, errors.Newf("unexpected status code: %d, failed to read response body: %w", resp.StatusCode, err)
		}
		return nil, 0, errors.Newf("unexpected status code: %d, response: %s", resp.StatusCode, respBody)
	}

	var smegaResponse kserveResponse
	err = s.json.NewDecoder(resp.Body).Decode(&smegaResponse)
	if err != nil {
		return nil, 0, err
	}

	res := codygateway.EmbeddingsResponse{
		Embeddings:      make([]codygateway.Embedding, items),
		Model:           request.Model,
		ModelDimensions: modelDimensions,
	}
	for i := 0; i*modelDimensions < len(smegaResponse.Outputs[0].Data); i++ {
		tensor := smegaResponse.Outputs[0].Data[i*modelDimensions : (i+1)*modelDimensions]
		res.Embeddings[i] = codygateway.Embedding{
			Data:  tensor,
			Index: i,
		}
	}

	// returning 0 means that SMEGA usage doesn't count towards the overall rate limit (which is what we want for now)
	return &res, 0, nil
}

// Based on https://github.com/kserve/kserve/blob/master/docs/predict-api/v2/required_api.md#inference-request-json-object
type kserveRequest struct {
	ID     string        `json:"id"`
	Inputs []kserveInput `json:"inputs"`
}

// Based on https://github.com/kserve/kserve/blob/master/docs/predict-api/v2/required_api.md#request-input
type kserveInput struct {
	Name     string   `json:"name"`
	Shape    []int    `json:"shape"`
	Datatype string   `json:"datatype"`
	Data     []string `json:"data"`
}

// Based on https://github.com/kserve/kserve/blob/master/docs/predict-api/v2/required_api.md#inference-response-json-object
type kserveResponse struct {
	ID           string         `json:"id"`
	ModelName    string         `json:"model_name"`
	ModelVersion string         `json:"model_version"`
	Outputs      []kserveOutput `json:"outputs"`
}

// Based on https://github.com/kserve/kserve/blob/master/docs/predict-api/v2/required_api.md#response-output
type kserveOutput struct {
	Name     string    `json:"name"`
	Shape    []int     `json:"shape"`
	Datatype string    `json:"datatype"`
	Data     []float32 `json:"data"`
}
