package completions

import (
	"context"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/cody-gateway/internal/tokenizer"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type flaggingConfig struct {
	// Phrases we look for in the prompt to consider it valid.
	// Each phrase is lower case.
	AllowedPromptPatterns []string
	// Phrases we look for in a flagged request to consider blocking the response.
	// Each phrase is lower case. Can be empty (to disable blocking).
	BlockedPromptPatterns []string
	// Phrases we look for in a request to collect data.
	// Each phrase is lower case. Can be empty (to disable data collection).
	PromptTokenFlaggingLimit       int
	PromptTokenBlockingLimit       int
	MaxTokensToSampleFlaggingLimit int
	ResponseTokenBlockingLimit     int
}
type flaggingRequest struct {
	FlattenedPrompt string
	MaxTokens       int
}
type flaggingResult struct {
	shouldBlock       bool
	blockedPhrase     *string
	reasons           []string
	promptPrefix      string
	maxTokensToSample int
	promptTokenCount  int
}

// isFlaggedRequest inspects the request and determines if it should be "flagged". This is how we
// perform basic abuse-detection and filtering. The implementation should err on the side of efficency,
// as the goal isn't for 100% accuracy. But to catch obvious abuse patterns, and let other backend
// systems do a more through review async.
func isFlaggedRequest(tk *tokenizer.Tokenizer, r flaggingRequest, cfg flaggingConfig) (*flaggingResult, error) {
	var reasons []string
	prompt := strings.ToLower(r.FlattenedPrompt)

	if hasValidPattern, _ := containsAny(prompt, cfg.AllowedPromptPatterns); len(cfg.AllowedPromptPatterns) > 0 && !hasValidPattern {
		reasons = append(reasons, "unknown_prompt")
	}

	// If this request has a very high token count for responses, then flag it.
	if r.MaxTokens > cfg.MaxTokensToSampleFlaggingLimit {
		reasons = append(reasons, "high_max_tokens_to_sample")
	}

	// If this prompt consists of a very large number of tokens, then flag it.
	tokens, err := tk.Tokenize(r.FlattenedPrompt)
	if err != nil {
		return &flaggingResult{}, errors.Wrap(err, "tokenize prompt")
	}
	tokenCount := len(tokens)

	if tokenCount > cfg.PromptTokenFlaggingLimit {
		reasons = append(reasons, "high_prompt_token_count")
	}

	if len(reasons) == 0 {
		return nil, nil
	}

	// The request has been flagged. Now we determine if it is serious enough to outright block the request.
	var blocked bool
	hasBlockedPhrase, phrase := containsAny(prompt, cfg.BlockedPromptPatterns)
	if tokenCount > cfg.PromptTokenBlockingLimit || r.MaxTokens > cfg.ResponseTokenBlockingLimit || hasBlockedPhrase {
		blocked = true
	}

	// Maximum number of characters of the prompt prefix we include in logs and telemetry.
	const logPromptPrefixLength = 250
	promptPrefix := r.FlattenedPrompt
	if len(promptPrefix) > logPromptPrefixLength {
		promptPrefix = promptPrefix[:logPromptPrefixLength]
	}

	res := &flaggingResult{
		reasons:           reasons,
		maxTokensToSample: r.MaxTokens,
		promptPrefix:      promptPrefix,
		promptTokenCount:  tokenCount,
		shouldBlock:       blocked,
	}
	if hasBlockedPhrase {
		res.blockedPhrase = &phrase
	}
	return res, nil
}

func (f *flaggingResult) IsFlagged() bool {
	return f != nil
}

func containsAny(prompt string, patterns []string) (bool, string) {
	prompt = strings.ToLower(prompt)
	for _, pattern := range patterns {
		if strings.Contains(prompt, pattern) {
			return true, pattern
		}
	}
	return false, ""
}

// requestBlockedError returns an error indicating that the request was blocked, including the trace ID.
func requestBlockedError(ctx context.Context) error {
	traceID := trace.FromContext(ctx).SpanContext().TraceID().String()
	return errors.Errorf("request blocked - if you think this is a mistake, please contact support@sourcegraph.com and reference this ID: %s", traceID)
}
