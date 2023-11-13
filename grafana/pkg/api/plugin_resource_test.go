package api

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"path/filepath"
	"strings"
	"testing"

	"github.com/grafana/grafana-azure-sdk-go/azsettings"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/localcache"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/tracing"
	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/backendplugin/coreplugin"
	pluginClient "github.com/grafana/grafana/pkg/plugins/manager/client"
	"github.com/grafana/grafana/pkg/plugins/manager/fakes"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/caching"
	datasources "github.com/grafana/grafana/pkg/services/datasources/fakes"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/oauthtoken/oauthtokentest"
	"github.com/grafana/grafana/pkg/services/pluginsintegration"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/pluginaccesscontrol"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/plugincontext"
	pluginSettings "github.com/grafana/grafana/pkg/services/pluginsintegration/pluginsettings/service"
	"github.com/grafana/grafana/pkg/services/quota/quotatest"
	fakeSecrets "github.com/grafana/grafana/pkg/services/secrets/fakes"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/tsdb/cloudwatch"
	testdatasource "github.com/grafana/grafana/pkg/tsdb/grafana-testdata-datasource"
	"github.com/grafana/grafana/pkg/web/webtest"
)

func TestCallResource(t *testing.T) {
	staticRootPath, err := filepath.Abs("../../public/")
	require.NoError(t, err)

	cfg := setting.NewCfg()
	cfg.StaticRootPath = staticRootPath
	cfg.Azure = &azsettings.AzureSettings{}

	coreRegistry := coreplugin.ProvideCoreRegistry(tracing.InitializeTracerForTest(), nil, &cloudwatch.CloudWatchService{}, nil, nil, nil, nil,
		nil, nil, nil, nil, testdatasource.ProvideService(), nil, nil, nil, nil, nil, nil)

	textCtx := pluginsintegration.CreateIntegrationTestCtx(t, cfg, coreRegistry)

	pcp := plugincontext.ProvideService(cfg, localcache.ProvideService(), textCtx.PluginStore, &datasources.FakeDataSourceService{},
		pluginSettings.ProvideService(db.InitTestDB(t), fakeSecrets.NewFakeSecretsService()), nil, nil)

	srv := SetupAPITestServer(t, func(hs *HTTPServer) {
		hs.Cfg = cfg
		hs.pluginContextProvider = pcp
		hs.QuotaService = quotatest.New(false, nil)
		hs.pluginStore = textCtx.PluginStore
		hs.pluginClient = textCtx.PluginClient
		hs.log = log.New("test")
	})

	t.Run("Test successful response is received for valid request", func(t *testing.T) {
		req := srv.NewPostRequest("/api/plugins/grafana-testdata-datasource/resources/test", strings.NewReader("{ \"test\": true }"))
		webtest.RequestWithSignedInUser(req, &user.SignedInUser{UserID: 1, OrgID: 1, Permissions: map[int64]map[string][]string{
			1: accesscontrol.GroupScopesByAction([]accesscontrol.Permission{
				{Action: pluginaccesscontrol.ActionAppAccess, Scope: pluginaccesscontrol.ScopeProvider.GetResourceAllScope()},
			}),
		}})
		resp, err := srv.SendJSON(req)
		require.NoError(t, err)

		b, err := io.ReadAll(resp.Body)
		require.NoError(t, err)

		var body = make(map[string]any)
		err = json.Unmarshal(b, &body)
		require.NoError(t, err)

		require.Equal(t, "Hello world from test datasource!", body["message"])
		require.NoError(t, resp.Body.Close())
		require.Equal(t, 200, resp.StatusCode)
	})
	pluginRegistry := fakes.NewFakePluginRegistry()
	require.NoError(t, pluginRegistry.Add(context.Background(), &plugins.Plugin{
		JSONData: plugins.JSONData{
			ID:      "grafana-testdata-datasource",
			Backend: true,
		},
	}))
	middlewares := pluginsintegration.CreateMiddlewares(cfg, &oauthtokentest.Service{}, tracing.InitializeTracerForTest(), &caching.OSSCachingService{}, &featuremgmt.FeatureManager{}, prometheus.DefaultRegisterer, pluginRegistry)
	pc, err := pluginClient.NewDecorator(&fakes.FakePluginClient{
		CallResourceHandlerFunc: backend.CallResourceHandlerFunc(func(ctx context.Context,
			req *backend.CallResourceRequest, sender backend.CallResourceResponseSender) error {
			return errors.New("something went wrong")
		}),
	}, middlewares...)
	require.NoError(t, err)

	srv = SetupAPITestServer(t, func(hs *HTTPServer) {
		hs.Cfg = cfg
		hs.pluginContextProvider = pcp
		hs.QuotaService = quotatest.New(false, nil)
		hs.pluginStore = textCtx.PluginStore
		hs.pluginClient = pc
		hs.log = log.New("test")
	})

	t.Run("Test error is properly propagated to API response", func(t *testing.T) {
		req := srv.NewGetRequest("/api/plugins/grafana-testdata-datasource/resources/scenarios")
		webtest.RequestWithSignedInUser(req, &user.SignedInUser{UserID: 1, OrgID: 1, Permissions: map[int64]map[string][]string{
			1: accesscontrol.GroupScopesByAction([]accesscontrol.Permission{
				{Action: pluginaccesscontrol.ActionAppAccess, Scope: pluginaccesscontrol.ScopeProvider.GetResourceAllScope()},
			}),
		}})
		resp, err := srv.SendJSON(req)
		require.NoError(t, err)

		body := new(strings.Builder)
		_, err = io.Copy(body, resp.Body)
		require.NoError(t, err)

		expectedBody := `{ "message": "Failed to call resource", "traceID": "" }`
		require.JSONEq(t, expectedBody, body.String())
		require.NoError(t, resp.Body.Close())
		require.Equal(t, 500, resp.StatusCode)
	})
}
