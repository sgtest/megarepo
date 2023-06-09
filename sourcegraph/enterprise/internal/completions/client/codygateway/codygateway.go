package codygateway

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"strings"

	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codygateway"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/client/anthropic"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/client/openai"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/completions/types"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const (
	// ProviderName is 'sourcegraph', since this is a Sourcegraph-provided service,
	// backed by Cody Gateway. This is the value accepted in site configuration.
	ProviderName    = "sourcegraph"
	DefaultEndpoint = "https://cody-gateway.sourcegraph.com"
)

// NewClient instantiates a completions provider backed by Sourcegraph's managed
// Cody Gateway service.
func NewClient(cli httpcli.Doer, endpoint, accessToken string) (types.CompletionsClient, error) {
	gatewayURL, err := url.Parse(endpoint)
	if err != nil {
		return nil, err
	}
	if accessToken == "" {
		return nil, errors.New("no access token is configured - make sure `licenseKey` is set, or provide an access token in `completions.accessToken`")
	}
	return &codyGatewayClient{
		upstream:    cli,
		gatewayURL:  gatewayURL,
		accessToken: accessToken,
	}, nil
}

type codyGatewayClient struct {
	upstream    httpcli.Doer
	gatewayURL  *url.URL
	accessToken string
}

func (c *codyGatewayClient) Stream(ctx context.Context, feature types.CompletionsFeature, requestParams types.CompletionRequestParameters, sendEvent types.SendCompletionEvent) error {
	cc, err := c.clientForParams(feature, &requestParams)
	if err != nil {
		return err
	}
	return cc.Stream(ctx, feature, requestParams, sendEvent)
}

func (c *codyGatewayClient) Complete(ctx context.Context, feature types.CompletionsFeature, requestParams types.CompletionRequestParameters) (*types.CompletionResponse, error) {
	cc, err := c.clientForParams(feature, &requestParams)
	if err != nil {
		return nil, err
	}
	// Passthrough error directly, ErrStatusNotOK should be implemented by the
	// underlying client.
	return cc.Complete(ctx, feature, requestParams)
}

func (c *codyGatewayClient) clientForParams(feature types.CompletionsFeature, requestParams *types.CompletionRequestParameters) (types.CompletionsClient, error) {
	// Extract provider and model from the Cody Gateway model format and override
	// the request parameter's model.
	provider, model := getProviderFromGatewayModel(strings.ToLower(requestParams.Model))
	requestParams.Model = model

	// Based on the provider, instantiate the appropriate client backed by a
	// gatewayDoer that authenticates against the Gateway's API.
	switch provider {
	case anthropic.ProviderName:
		return anthropic.NewClient(gatewayDoer(c.upstream, feature, c.gatewayURL, c.accessToken, "/v1/completions/anthropic"), "", ""), nil
	case openai.ProviderName:
		return openai.NewClient(gatewayDoer(c.upstream, feature, c.gatewayURL, c.accessToken, "/v1/completions/openai"), "", ""), nil
	case "":
		return nil, errors.Newf("no provider provided in model %s - a model in the format '$PROVIDER/$MODEL_NAME' is expected", model)
	default:
		return nil, errors.Newf("no client known for upstream provider %s", provider)
	}
}

// getProviderFromGatewayModel extracts the model provider from Cody Gateway
// configuration's expected model naming format, "$PROVIDER/$MODEL_NAME".
// If a prefix isn't present, the whole value is assumed to be the model.
func getProviderFromGatewayModel(gatewayModel string) (provider string, model string) {
	parts := strings.SplitN(gatewayModel, "/", 2)
	if len(parts) < 2 {
		return "", parts[0] // assume it's the provider that's missing, not the model.
	}
	return parts[0], parts[1]
}

// gatewayDoer redirects requests to Cody Gateway with all prerequisite headers.
func gatewayDoer(upstream httpcli.Doer, feature types.CompletionsFeature, gatewayURL *url.URL, accessToken, path string) httpcli.Doer {
	return httpcli.DoerFunc(func(req *http.Request) (*http.Response, error) {
		req.Host = gatewayURL.Host
		req.URL = gatewayURL
		req.URL.Path = path
		req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", accessToken))
		req.Header.Set(codygateway.FeatureHeaderName, string(feature))

		resp, err := upstream.Do(req)

		// If we get a repsonse, record Cody Gateway's x-trace response header,
		// so that we can link up to an event on our end if needed.
		if resp != nil && resp.Header != nil {
			if span := trace.SpanFromContext(req.Context()); span.SpanContext().IsValid() {
				// Would be cool if we can make an OTEL trace link instead, but
				// adding a link after a span has started is not supported yet:
				// https://github.com/open-telemetry/opentelemetry-specification/issues/454
				span.SetAttributes(attribute.String("cody-gateway.x-trace", resp.Header.Get("X-Trace")))
				span.SetAttributes(attribute.String("cody-gateway.x-trace-span", resp.Header.Get("X-Trace-Span")))
			}
		}

		return resp, err
	})
}
