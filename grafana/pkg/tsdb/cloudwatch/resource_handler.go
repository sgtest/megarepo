package cloudwatch

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/backend/resource/httpadapter"

	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/tsdb/cloudwatch/routes"
)

func (e *cloudWatchExecutor) newResourceMux() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("/ebs-volume-ids", handleResourceReq(e.handleGetEbsVolumeIds))
	mux.HandleFunc("/ec2-instance-attribute", handleResourceReq(e.handleGetEc2InstanceAttribute))
	mux.HandleFunc("/resource-arns", handleResourceReq(e.handleGetResourceArns))
	mux.HandleFunc("/log-groups", routes.ResourceRequestMiddleware(routes.LogGroupsHandler, logger, e.getRequestContext))
	mux.HandleFunc("/metrics", routes.ResourceRequestMiddleware(routes.MetricsHandler, logger, e.getRequestContext))
	mux.HandleFunc("/dimension-values", routes.ResourceRequestMiddleware(routes.DimensionValuesHandler, logger, e.getRequestContext))
	mux.HandleFunc("/dimension-keys", routes.ResourceRequestMiddleware(routes.DimensionKeysHandler, logger, e.getRequestContext))
	mux.HandleFunc("/accounts", routes.ResourceRequestMiddleware(routes.AccountsHandler, logger, e.getRequestContext))
	mux.HandleFunc("/namespaces", routes.ResourceRequestMiddleware(routes.NamespacesHandler, logger, e.getRequestContext))
	mux.HandleFunc("/log-group-fields", routes.ResourceRequestMiddleware(routes.LogGroupFieldsHandler, logger, e.getRequestContext))
	mux.HandleFunc("/external-id", routes.ResourceRequestMiddleware(routes.ExternalIdHandler, logger, e.getRequestContext))

	// feature is enabled by default, just putting behind a feature flag in case of unexpected bugs
	if e.features.IsEnabledGlobally(featuremgmt.FlagCloudwatchNewRegionsHandler) {
		mux.HandleFunc("/regions", routes.ResourceRequestMiddleware(routes.RegionsHandler, logger, e.getRequestContext))
	} else {
		mux.HandleFunc("/regions", handleResourceReq(e.handleGetRegions))
	}

	// remove this once AWS's Cross Account Observability is supported in GovCloud
	mux.HandleFunc("/legacy-log-groups", handleResourceReq(e.handleGetLogGroups))

	return mux
}

type handleFn func(ctx context.Context, pluginCtx backend.PluginContext, parameters url.Values) ([]suggestData, error)

func handleResourceReq(handleFunc handleFn) func(rw http.ResponseWriter, req *http.Request) {
	return func(rw http.ResponseWriter, req *http.Request) {
		ctx := req.Context()
		pluginContext := httpadapter.PluginConfigFromContext(ctx)
		err := req.ParseForm()
		if err != nil {
			writeResponse(rw, http.StatusBadRequest, fmt.Sprintf("unexpected error %v", err))
			return
		}
		data, err := handleFunc(ctx, pluginContext, req.URL.Query())
		if err != nil {
			writeResponse(rw, http.StatusBadRequest, fmt.Sprintf("unexpected error %v", err))
			return
		}
		body, err := json.Marshal(data)
		if err != nil {
			writeResponse(rw, http.StatusBadRequest, fmt.Sprintf("unexpected error %v", err))
			return
		}
		rw.WriteHeader(http.StatusOK)
		_, err = rw.Write(body)
		if err != nil {
			logger.Error("Unable to write HTTP response", "error", err)
			return
		}
	}
}

func writeResponse(rw http.ResponseWriter, code int, msg string) {
	rw.WriteHeader(code)
	_, err := rw.Write([]byte(msg))
	if err != nil {
		logger.Error("Unable to write HTTP response", "error", err)
	}
}
