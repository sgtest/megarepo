package embeddings

import (
	"context"
	"encoding/json"
	"net/http"

	"golang.org/x/exp/slices"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/cody-gateway/internal/actor"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
)

type ModelName string

const (
	ModelNameOpenAIAda ModelName = "openai/text-embedding-ada-002"
)

type EmbeddingsClient interface {
	GenerateEmbeddings(context.Context, codygateway.EmbeddingsRequest) (_ *codygateway.EmbeddingsResponse, consumedTokens int, _ error)
}

type ModelFactory interface {
	ForModel(model string) (_ EmbeddingsClient, ok bool)
}

type ModelFactoryMap map[ModelName]EmbeddingsClient

func (mf ModelFactoryMap) ForModel(model string) (EmbeddingsClient, bool) {
	c, ok := mf[ModelName(model)]
	return c, ok
}

func NewListHandler() http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		act := actor.FromContext(r.Context())

		modelEnabled := func(model ModelName) bool {
			rl, ok := act.RateLimits[codygateway.FeatureEmbeddings]
			if !act.AccessEnabled || !ok || !rl.IsValid() {
				return false
			}
			return slices.Contains(rl.AllowedModels, string(model))
		}

		models := modelsResponse{
			// Just a hardcoded list for now.
			{
				Enabled:    modelEnabled(ModelNameOpenAIAda),
				Name:       string(ModelNameOpenAIAda),
				Dimensions: 1536,
				Deprecated: false,
			},
		}
		_ = json.NewEncoder(w).Encode(models)
	})
}

type model struct {
	Name       string `json:"name"`
	Dimensions int    `json:"dimensions"`
	Enabled    bool   `json:"enabled"`
	Deprecated bool   `json:"deprecated"`
}

type modelsResponse []model
