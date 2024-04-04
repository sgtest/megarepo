package tokenizer

import (
	_ "embed"
	"strings"

	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/pkoukk/tiktoken-go"
)

// Define constants for model types
const (
	AnthropicModel = "anthropic"
	AzureModel     = "azure"
	OpenAIModel    = "openai"
)

// Tokenizer is an interface that all tokenizer implementations must satisfy.
type Tokenizer interface {
	Tokenize(text string) ([]int, error)
}

type tiktokenTokenizer struct {
	tk    *tiktoken.Tiktoken
	model string
}

func (t *tiktokenTokenizer) Tokenize(text string) ([]int, error) {
	return t.tk.Encode(text, []string{"all"}, nil), nil
}

// NewTokenizer returns a Tokenizer instance based on the provided model.
func NewTokenizer(model string) (Tokenizer, error) {
	switch {
	case strings.Contains(model, AnthropicModel):
		return newAnthropicClaudeTokenizer(model)
	case strings.Contains(model, AzureModel), strings.Contains(model, OpenAIModel):
		// Returning a tiktokenTokenizer for models related to "azure" or "openai"
		return newOpenAITokenizer(model)
	default:
		return nil, errors.New("tokenizer not found for this model")
	}
}

func newOpenAITokenizer(model string) (*tiktokenTokenizer, error) {
	// Remove "azure" or "openai" prefix from the model string
	model = strings.NewReplacer(AzureModel+"/", "", OpenAIModel+"/", "").Replace(model)

	tkm, err := tiktoken.EncodingForModel(model)
	if err != nil {
		return nil, errors.Newf("tiktoken encoding error: %v", err)
	}

	return &tiktokenTokenizer{
		tk:    tkm,
		model: model,
	}, nil
}
