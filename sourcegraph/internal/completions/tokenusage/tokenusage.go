package tokenusage

import (
	"fmt"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/completions/tokenizer"
	"github.com/sourcegraph/sourcegraph/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Manager struct {
	Cache *rcache.Cache
}

type ModelData struct {
	Description string  `json:"description"`
	Tokens      float64 `json:"tokens"`
}

func NewManager() *Manager {
	return &Manager{
		Cache: rcache.NewWithRedisStore("LLMUsage"),
	}
}

type Provider string

const (
	OpenAI      Provider = "openai"
	AzureOpenAI Provider = "azureopenai"
	AwsBedrock  Provider = "awsbedrock"
	Anthropic   Provider = "anthropic"
)

func (m *Manager) TokenizeAndCalculateUsage(inputMessages []types.Message, outputText, model, feature string, provider Provider) error {
	tokenizer, err := tokenizer.NewCL100kBaseTokenizer()
	if err != nil {
		return errors.Newf("failed to create tokenizer: %w", err)
	}

	numInputTokens, err := tokenizer.NumTokenizeFromMessages(inputMessages)
	if err != nil {
		return errors.Newf("failed to tokenize input text: %w", err)
	}

	outputTokens, err := tokenizer.Tokenize(outputText)
	if err != nil {
		return errors.Newf("failed to tokenize output text: %w", err)
	}

	baseKey := fmt.Sprintf("%s:%s:%s:", provider, model, feature)

	if err := m.updateTokenCounts(baseKey+"input", int64(numInputTokens)); err != nil {
		return errors.Newf("failed to update input token counts: %w", err)
	}
	if err := m.updateTokenCounts(baseKey+"output", int64(len(outputTokens))); err != nil {
		return errors.Newf("failed to update output token counts: %w", err)
	}
	return nil
}

func (m *Manager) UpdateTokenCountsFromModelUsage(inputTokens, outputTokens int, model, feature string, provider Provider) error {
	baseKey := fmt.Sprintf("%s:%s:%s:", provider, model, feature)

	if err := m.updateTokenCounts(baseKey+"input", int64(inputTokens)); err != nil {
		return errors.Newf("failed to update input token counts: %w", err)
	}
	if err := m.updateTokenCounts(baseKey+"output", int64(outputTokens)); err != nil {
		return errors.Newf("failed to update output token counts: %w", err)
	}
	return nil
}

func (m *Manager) updateTokenCounts(key string, tokenCount int64) error {
	if _, err := m.Cache.IncrByInt64(key, tokenCount); err != nil {
		return errors.Newf("failed to increment token count for key %s: %w", key, err)
	}
	return nil
}

func (m *Manager) RetrieveAndResetTokenUsageData() (map[string]interface{}, error) {
	tokenUsageData, err := m.fetchTokenUsageData(true) // true to decrement counts
	if err != nil {
		return nil, err
	}

	// Grouping token usage data under a 'models' key
	modelsData := make([]ModelData, 0, len(tokenUsageData))
	for model, tokens := range tokenUsageData {
		modelData := ModelData{
			Description: model, // Assuming 'model' contains the description
			Tokens:      tokens,
		}
		modelsData = append(modelsData, modelData)
	}
	result := map[string]interface{}{
		"llm_usage": []map[string]interface{}{
			{"models": modelsData},
		},
	}
	return result, nil
}

func (m *Manager) FetchTokenUsageDataForAnalysis() (map[string]float64, error) {
	return m.fetchTokenUsageData(false) // false means do not decrement counts
}

// fetchTokenUsageData retrieves token usage data, optionally decrementing token counts.
func (m *Manager) fetchTokenUsageData(decrement bool) (map[string]float64, error) {
	allKeys := m.Cache.ListAllKeys()
	tokenUsageData := make(map[string]float64)

	for _, key := range allKeys {
		modelName, value, err := m.getModelNameAndValue(key, decrement)
		if err != nil {
			continue // Skip keys with errors
		}

		tokenUsageData[modelName] = float64(value)
	}

	return tokenUsageData, nil
}

// getModelNameAndValue extracts the model name and value from a key, optionally decrementing the token count.
func (m *Manager) getModelNameAndValue(key string, decrement bool) (string, int64, error) {
	parts := strings.SplitN(key, "LLMUsage:", 2)
	if len(parts) < 2 {
		return "", 0, errors.New("invalid key format")
	}
	modelName := parts[1]

	value, found, err := m.Cache.GetInt64(modelName)
	if err != nil || !found {
		return "", 0, err // Skip keys that are not found or have conversion errors
	}

	if decrement {
		if _, err := m.Cache.DecrByInt64(modelName, value); err != nil {
			return "", 0, err
		}
	}

	return modelName, value, nil
}
