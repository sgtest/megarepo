package clientmiddleware

import (
	"context"

	"github.com/grafana/grafana-plugin-sdk-go/backend"

	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/services/contexthandler"
)

const forwardIDHeaderName = "X-Grafana-Id"

// NewForwardIDMiddleware creates a new plugins.ClientMiddleware that will
// set grafana id header on outgoing plugins.Client requests if the
// feature toggle FlagIdForwarding is enabled
func NewForwardIDMiddleware() plugins.ClientMiddleware {
	return plugins.ClientMiddlewareFunc(func(next plugins.Client) plugins.Client {
		return &ForwardIDMiddleware{
			next: next,
		}
	})
}

type ForwardIDMiddleware struct {
	next plugins.Client
}

func (m *ForwardIDMiddleware) applyToken(ctx context.Context, pCtx backend.PluginContext, req backend.ForwardHTTPHeaders) error {
	reqCtx := contexthandler.FromContext(ctx)
	// no HTTP request context => skip middleware
	if req == nil || reqCtx == nil || reqCtx.SignedInUser == nil {
		return nil
	}

	// token will only be present if faeturemgmt.FlagIdForwarding is enabled
	if token := reqCtx.SignedInUser.GetIDToken(); token != "" {
		req.SetHTTPHeader(forwardIDHeaderName, token)
	}

	return nil
}

func (m *ForwardIDMiddleware) QueryData(ctx context.Context, req *backend.QueryDataRequest) (*backend.QueryDataResponse, error) {
	if req == nil {
		return m.next.QueryData(ctx, req)
	}

	err := m.applyToken(ctx, req.PluginContext, req)
	if err != nil {
		return nil, err
	}

	return m.next.QueryData(ctx, req)
}

func (m *ForwardIDMiddleware) CallResource(ctx context.Context, req *backend.CallResourceRequest, sender backend.CallResourceResponseSender) error {
	if req == nil {
		return m.next.CallResource(ctx, req, sender)
	}

	err := m.applyToken(ctx, req.PluginContext, req)
	if err != nil {
		return err
	}

	return m.next.CallResource(ctx, req, sender)
}

func (m *ForwardIDMiddleware) CheckHealth(ctx context.Context, req *backend.CheckHealthRequest) (*backend.CheckHealthResult, error) {
	if req == nil {
		return m.next.CheckHealth(ctx, req)
	}

	err := m.applyToken(ctx, req.PluginContext, req)
	if err != nil {
		return nil, err
	}

	return m.next.CheckHealth(ctx, req)
}

func (m *ForwardIDMiddleware) CollectMetrics(ctx context.Context, req *backend.CollectMetricsRequest) (*backend.CollectMetricsResult, error) {
	return m.next.CollectMetrics(ctx, req)
}

func (m *ForwardIDMiddleware) SubscribeStream(ctx context.Context, req *backend.SubscribeStreamRequest) (*backend.SubscribeStreamResponse, error) {
	return m.next.SubscribeStream(ctx, req)
}

func (m *ForwardIDMiddleware) PublishStream(ctx context.Context, req *backend.PublishStreamRequest) (*backend.PublishStreamResponse, error) {
	return m.next.PublishStream(ctx, req)
}

func (m *ForwardIDMiddleware) RunStream(ctx context.Context, req *backend.RunStreamRequest, sender *backend.StreamSender) error {
	return m.next.RunStream(ctx, req, sender)
}
