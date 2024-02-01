package resource

import (
	"bytes"
	"context"
	"fmt"
	"net/http"

	"github.com/grafana/grafana-azure-sdk-go/util/maputil"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/backend/log"

	"github.com/grafana/grafana/pkg/tsdb/prometheus/client"
	"github.com/grafana/grafana/pkg/tsdb/prometheus/utils"
)

type Resource struct {
	promClient *client.Client
	log        log.Logger
}

func New(
	httpClient *http.Client,
	settings backend.DataSourceInstanceSettings,
	plog log.Logger,
) (*Resource, error) {
	jsonData, err := utils.GetJsonData(settings)
	if err != nil {
		return nil, err
	}
	httpMethod, _ := maputil.GetStringOptional(jsonData, "httpMethod")

	if httpMethod == "" {
		httpMethod = http.MethodPost
	}

	return &Resource{
		log:        plog,
		promClient: client.NewClient(httpClient, httpMethod, settings.URL),
	}, nil
}

func (r *Resource) Execute(ctx context.Context, req *backend.CallResourceRequest) (*backend.CallResourceResponse, error) {
	r.log.FromContext(ctx).Debug("Sending resource query", "URL", req.URL)
	resp, err := r.promClient.QueryResource(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("error querying resource: %v", err)
	}

	// frontend sets the X-Grafana-Cache with the desired response cache control value
	if len(req.GetHTTPHeaders().Get("X-Grafana-Cache")) > 0 {
		resp.Header.Set("X-Grafana-Cache", "y")
		resp.Header.Set("Cache-Control", req.GetHTTPHeaders().Get("X-Grafana-Cache"))
	}

	defer func() {
		tmpErr := resp.Body.Close()
		if tmpErr != nil && err == nil {
			err = tmpErr
		}
	}()

	var buf bytes.Buffer
	// Should be more efficient than ReadAll. See https://github.com/prometheus/client_golang/pull/976
	_, err = buf.ReadFrom(resp.Body)
	body := buf.Bytes()
	if err != nil {
		return nil, err
	}
	callResponse := &backend.CallResourceResponse{
		Status:  resp.StatusCode,
		Headers: resp.Header,
		Body:    body,
	}

	return callResponse, err
}

func (r *Resource) DetectVersion(ctx context.Context, req *backend.CallResourceRequest) (*backend.CallResourceResponse, error) {
	newReq := &backend.CallResourceRequest{
		PluginContext: req.PluginContext,
		Path:          "/api/v1/status/buildinfo",
	}

	return r.Execute(ctx, newReq)
}
