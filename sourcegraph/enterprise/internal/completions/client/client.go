package client

import (
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/client/anthropic"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/client/codygateway"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/client/openai"
	"github.com/sourcegraph/sourcegraph/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func Get(endpoint string, provider conftypes.CompletionsProviderName, accessToken string) (types.CompletionsClient, error) {
	switch provider {
	case conftypes.CompletionsProviderNameAnthropic:
		return anthropic.NewClient(httpcli.ExternalDoer, endpoint, accessToken), nil
	case conftypes.CompletionsProviderNameOpenAI:
		return openai.NewClient(httpcli.ExternalDoer, endpoint, accessToken), nil
	case conftypes.CompletionsProviderNameSourcegraph:
		return codygateway.NewClient(httpcli.ExternalDoer, endpoint, accessToken)
	default:
		return nil, errors.Newf("unknown completion stream provider: %s", provider)
	}
}
