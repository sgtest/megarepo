package usagestats

import (
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func GetCodyProviders() (*types.CodyProviders, error) {
	siteConfig := conf.SiteConfig()
	completionsConfig := conf.GetCompletionsConfig(siteConfig)
	embeddingsConfig := conf.GetEmbeddingsConfig(siteConfig)
	providers := types.CodyProviders{}
	if completionsConfig != nil {
		providers.Completions = &types.CodyCompletionProvider{
			Provider: completionsConfig.Provider,
		}
		if completionsConfig.Provider == conftypes.CompletionsProviderNameSourcegraph {
			providers.Completions.ChatModel = completionsConfig.ChatModel
			providers.Completions.CompletionModel = completionsConfig.CompletionModel
			providers.Completions.FastChatModel = completionsConfig.FastChatModel
		}
	}
	if embeddingsConfig != nil {
		providers.Embeddings = &types.CodyEmbeddingsProvider{
			Provider: embeddingsConfig.Provider,
		}
		if embeddingsConfig.Provider == conftypes.EmbeddingsProviderNameSourcegraph {
			providers.Embeddings.Model = embeddingsConfig.Model
		}
	}
	return &providers, nil
}
