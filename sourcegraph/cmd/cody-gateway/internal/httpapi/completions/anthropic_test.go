package completions

import (
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/hexops/autogold/v2"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/shared/config"

	"github.com/sourcegraph/sourcegraph/internal/completions/tokenizer"
)

func TestIsFlaggedAnthropicRequest(t *testing.T) {
	validPreamble := "You are cody-gateway."

	cfg := config.FlaggingConfig{
		PromptTokenFlaggingLimit:       18000,
		PromptTokenBlockingLimit:       20000,
		MaxTokensToSample:              0, // Not used within isFlaggedRequest.
		MaxTokensToSampleFlaggingLimit: 4000,
		ResponseTokenBlockingLimit:     4000,
		RequestBlockingEnabled:         true,
	}
	cfgWithPreamble := config.FlaggingConfig{
		PromptTokenFlaggingLimit:       18000,
		PromptTokenBlockingLimit:       20000,
		MaxTokensToSample:              0, // Not used within isFlaggedRequest.
		MaxTokensToSampleFlaggingLimit: 4000,
		ResponseTokenBlockingLimit:     4000,
		RequestBlockingEnabled:         true,
		AllowedPromptPatterns:          []string{strings.ToLower(validPreamble)},
	}
	tk, err := tokenizer.NewCL100kBaseTokenizer()
	require.NoError(t, err)

	// Helper function for calling the AnthropicHandlerMethod's shouldFlagRequest, using the supplied
	// request and configuration.
	callShouldFlagRequest := func(t *testing.T, ar anthropicRequest, flaggingConfig config.FlaggingConfig) (*flaggingResult, error) {
		t.Helper()
		anthropicUpstream := &AnthropicHandlerMethods{
			anthropicTokenizer: tk,
			config: config.AnthropicConfig{
				FlaggingConfig: flaggingConfig,
			},
		}
		ctx := context.Background()
		logger := logtest.NoOp(t)
		return anthropicUpstream.shouldFlagRequest(ctx, logger, ar)
	}

	t.Run("missing known preamble", func(t *testing.T) {
		ar := anthropicRequest{Model: "claude-2", Prompt: "some prompt without known preamble"}
		result, err := callShouldFlagRequest(t, ar, cfgWithPreamble)
		require.NoError(t, err)
		require.True(t, result.IsFlagged())
		require.False(t, result.shouldBlock)
		require.Contains(t, result.reasons, "unknown_prompt")
	})

	t.Run("preamble not configured ", func(t *testing.T) {
		ar := anthropicRequest{Model: "claude-2", Prompt: "some prompt without known preamble"}
		result, err := callShouldFlagRequest(t, ar, cfg)
		require.NoError(t, err)
		require.False(t, result.IsFlagged())
	})

	t.Run("high max tokens to sample", func(t *testing.T) {
		ar := anthropicRequest{Model: "claude-2", MaxTokensToSample: 10000, Prompt: validPreamble}
		result, err := callShouldFlagRequest(t, ar, cfg)
		require.NoError(t, err)
		require.True(t, result.IsFlagged())
		require.True(t, result.shouldBlock)
		require.Contains(t, result.reasons, "high_max_tokens_to_sample")
		require.Equal(t, int32(result.maxTokensToSample), ar.MaxTokensToSample)
	})

	t.Run("high prompt token count (above block limit)", func(t *testing.T) {
		tokenLengths, err := tk.Tokenize(validPreamble)
		require.NoError(t, err)

		validPreambleTokens := len(tokenLengths)
		longPrompt := strings.Repeat("word ", cfg.PromptTokenFlaggingLimit+1)
		ar := anthropicRequest{Model: "claude-2", Prompt: validPreamble + " " + longPrompt}
		result, err := callShouldFlagRequest(t, ar, cfgWithPreamble)
		require.NoError(t, err)
		require.True(t, result.IsFlagged())
		require.False(t, result.shouldBlock)
		require.Contains(t, result.reasons, "high_prompt_token_count")
		require.Equal(t, result.promptTokenCount, validPreambleTokens+1+cfg.PromptTokenFlaggingLimit+1, cfg)
	})

	t.Run("high prompt token count and bad phrase", func(t *testing.T) {
		cfgWithBadPhrase := cfgWithPreamble
		cfgWithBadPhrase.BlockedPromptPatterns = []string{"bad phrase"}
		longPrompt := strings.Repeat("word ", cfg.PromptTokenFlaggingLimit+1)
		ar := anthropicRequest{Model: "claude-2", Prompt: validPreamble + " " + longPrompt + "bad phrase"}
		result, err := callShouldFlagRequest(t, ar, cfgWithBadPhrase)
		require.NoError(t, err)
		require.True(t, result.IsFlagged())
		require.True(t, result.shouldBlock)
	})

	t.Run("low prompt token count and bad phrase", func(t *testing.T) {
		cfgWithBadPhrase := cfgWithPreamble
		cfgWithBadPhrase.BlockedPromptPatterns = []string{"bad phrase"}
		longPrompt := strings.Repeat("word ", 5)
		ar := anthropicRequest{Model: "claude-2", Prompt: validPreamble + " " + longPrompt + "bad phrase"}
		result, err := callShouldFlagRequest(t, ar, cfgWithBadPhrase)
		require.NoError(t, err)
		// As of https://sourcegraph.slack.com/archives/C06062P5TS5/p1716896478893949?thread_ts=1716475553.409679&cid=C06062P5TS5), we consider bad phrases to be sufficient for flagging (and blocking)
		require.True(t, result.IsFlagged())
		require.True(t, result.shouldBlock)
	})

	t.Run("high prompt token count (above block limit)", func(t *testing.T) {
		tokenLengths, err := tk.Tokenize(validPreamble)
		require.NoError(t, err)

		validPreambleTokens := len(tokenLengths)
		longPrompt := strings.Repeat("word ", cfg.PromptTokenBlockingLimit+1)
		ar := anthropicRequest{Model: "claude-2", Prompt: validPreamble + " " + longPrompt}
		result, err := callShouldFlagRequest(t, ar, cfgWithPreamble)
		require.NoError(t, err)
		require.True(t, result.IsFlagged())
		require.True(t, result.shouldBlock)
		require.Contains(t, result.reasons, "high_prompt_token_count")
		require.Equal(t, result.promptTokenCount, validPreambleTokens+1+cfg.PromptTokenBlockingLimit+1)
	})
}

func TestAnthropicRequestJSON(t *testing.T) {
	tk, err := tokenizer.NewCL100kBaseTokenizer()
	require.NoError(t, err)

	ar := anthropicRequest{Prompt: "Hello world"}
	_, _ = ar.GetPromptTokenCount(tk) // hydrate values that should not be marshalled

	b, err := json.MarshalIndent(ar, "", "\t")
	require.NoError(t, err)

	autogold.Expect(`{
"prompt": "Hello world",
"model": "",
"max_tokens_to_sample": 0
}`).Equal(t, string(b))
}

func TestAnthropicRequestGetPromptTokenCount(t *testing.T) {
	tk, err := tokenizer.NewCL100kBaseTokenizer()
	require.NoError(t, err)

	originalRequest := anthropicRequest{Prompt: "Hello world"}

	t.Run("values are hydrated", func(t *testing.T) {
		count, err := originalRequest.GetPromptTokenCount(tk)
		require.NoError(t, err)
		assert.Equal(t, originalRequest.promptTokens.count, count)
		assert.Nil(t, originalRequest.promptTokens.err)
	})

	t.Run("values are not recomputed", func(t *testing.T) {
		newRequest := originalRequest // copy
		// Contrived example, we never update the prompt.
		newRequest.Prompt = "Hello again! This is a much longer prompt now"
		count2, err := newRequest.GetPromptTokenCount(tk)
		require.NoError(t, err)
		assert.Equal(t, originalRequest.promptTokens.count, count2, "token count should be unchanged")
	})
}
