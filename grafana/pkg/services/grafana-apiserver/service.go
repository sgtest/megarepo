package grafanaapiserver

import (
	"context"
	"fmt"
	"net/http"
	"path"
	"strconv"

	"github.com/go-logr/logr"
	"github.com/grafana/dskit/services"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/apimachinery/pkg/apis/meta/v1/unstructured"
	"k8s.io/apimachinery/pkg/runtime"
	"k8s.io/apimachinery/pkg/runtime/schema"
	"k8s.io/apimachinery/pkg/runtime/serializer"
	"k8s.io/apiserver/pkg/authorization/authorizer"
	openapinamer "k8s.io/apiserver/pkg/endpoints/openapi"
	"k8s.io/apiserver/pkg/endpoints/responsewriter"
	genericapiserver "k8s.io/apiserver/pkg/server"
	"k8s.io/apiserver/pkg/server/options"
	"k8s.io/apiserver/pkg/util/openapi"
	"k8s.io/client-go/kubernetes/scheme"
	clientrest "k8s.io/client-go/rest"
	"k8s.io/client-go/tools/clientcmd"
	clientcmdapi "k8s.io/client-go/tools/clientcmd/api"
	"k8s.io/component-base/logs"
	"k8s.io/klog/v2"

	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/middleware"
	"github.com/grafana/grafana/pkg/modules"
	"github.com/grafana/grafana/pkg/registry"
	contextmodel "github.com/grafana/grafana/pkg/services/contexthandler/model"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/web"
)

var (
	_ Service                    = (*service)(nil)
	_ RestConfigProvider         = (*service)(nil)
	_ registry.BackgroundService = (*service)(nil)
	_ registry.CanBeDisabled     = (*service)(nil)

	Scheme = runtime.NewScheme()
	Codecs = serializer.NewCodecFactory(Scheme)

	unversionedVersion = schema.GroupVersion{Group: "", Version: "v1"}
	unversionedTypes   = []runtime.Object{
		&metav1.Status{},
		&metav1.WatchEvent{},
		&metav1.APIVersions{},
		&metav1.APIGroupList{},
		&metav1.APIGroup{},
		&metav1.APIResourceList{},
	}
)

func init() {
	// we need to add the options to empty v1
	metav1.AddToGroupVersion(Scheme, schema.GroupVersion{Group: "", Version: "v1"})
	Scheme.AddUnversionedTypes(unversionedVersion, unversionedTypes...)
}

type Service interface {
	services.NamedService
	registry.BackgroundService
	registry.CanBeDisabled
}

type APIRegistrar interface {
	RegisterAPI(builder APIGroupBuilder)
}

type RestConfigProvider interface {
	GetRestConfig() *clientrest.Config
}

type service struct {
	*services.BasicService

	config     *config
	restConfig *clientrest.Config

	stopCh    chan struct{}
	stoppedCh chan error

	rr       routing.RouteRegister
	handler  web.Handler
	builders []APIGroupBuilder

	authorizer authorizer.Authorizer
}

func ProvideService(
	cfg *setting.Cfg,
	rr routing.RouteRegister,
	authz authorizer.Authorizer,
) (*service, error) {
	s := &service{
		config:     newConfig(cfg),
		rr:         rr,
		stopCh:     make(chan struct{}),
		builders:   []APIGroupBuilder{},
		authorizer: authz,
	}

	// This will be used when running as a dskit service
	s.BasicService = services.NewBasicService(s.start, s.running, nil).WithName(modules.GrafanaAPIServer)

	// TODO: this is very hacky
	// We need to register the routes in ProvideService to make sure
	// the routes are registered before the Grafana HTTP server starts.
	proxyHandler := func(k8sRoute routing.RouteRegister) {
		handler := func(c *contextmodel.ReqContext) {
			if s.handler == nil {
				c.Resp.WriteHeader(404)
				_, _ = c.Resp.Write([]byte("Not found"))
				return
			}

			if handle, ok := s.handler.(func(c *contextmodel.ReqContext)); ok {
				handle(c)
				return
			}
		}
		k8sRoute.Any("/", middleware.ReqSignedIn, handler)
		k8sRoute.Any("/*", middleware.ReqSignedIn, handler)
	}

	s.rr.Group("/apis", proxyHandler)
	s.rr.Group("/apiserver-metrics", proxyHandler)
	s.rr.Group("/openapi", proxyHandler)

	return s, nil
}

func (s *service) GetRestConfig() *clientrest.Config {
	return s.restConfig
}

func (s *service) IsDisabled() bool {
	return !s.config.enabled
}

// Run is an adapter for the BackgroundService interface.
func (s *service) Run(ctx context.Context) error {
	if err := s.start(ctx); err != nil {
		return err
	}
	return s.running(ctx)
}

func (s *service) RegisterAPI(builder APIGroupBuilder) {
	s.builders = append(s.builders, builder)
}

func (s *service) start(ctx context.Context) error {
	logger := logr.New(newLogAdapter(s.config.logLevel))
	klog.SetLoggerWithOptions(logger, klog.ContextualLogger(true))
	if _, err := logs.GlogSetter(strconv.Itoa(s.config.logLevel)); err != nil {
		logger.Error(err, "failed to set log level")
	}

	o := options.NewRecommendedOptions("", unstructured.UnstructuredJSONScheme)
	o.SecureServing.BindAddress = s.config.ip
	o.SecureServing.BindPort = s.config.port
	o.Authentication.RemoteKubeConfigFileOptional = true
	o.Authorization.RemoteKubeConfigFileOptional = true
	o.Etcd.StorageConfig.Transport.ServerList = s.config.etcdServers

	o.Admission = nil
	o.CoreAPI = nil
	if len(o.Etcd.StorageConfig.Transport.ServerList) == 0 {
		o.Etcd = nil
	}

	if err := o.Validate(); len(err) > 0 {
		return err[0]
	}

	serverConfig := genericapiserver.NewRecommendedConfig(Codecs)
	serverConfig.ExternalAddress = s.config.host

	if s.config.devMode {
		// SecureServingOptions is used when the apiserver needs it's own listener.
		// this is not needed in production, but it's useful for development kubectl access.
		if err := o.SecureServing.ApplyTo(&serverConfig.SecureServing, &serverConfig.LoopbackClientConfig); err != nil {
			return err
		}
		// AuthenticationOptions is needed to authenticate requests from kubectl in dev mode.
		if err := o.Authentication.ApplyTo(&serverConfig.Authentication, serverConfig.SecureServing, serverConfig.OpenAPIConfig); err != nil {
			return err
		}
	}

	// override ExternalAddress and LoopbackClientConfig in prod mode.
	// in dev mode we want to use the loopback client config
	// and address provided by SecureServingOptions.
	if !s.config.devMode {
		serverConfig.ExternalAddress = s.config.host
		serverConfig.LoopbackClientConfig = &clientrest.Config{
			Host: s.config.apiURL,
			TLSClientConfig: clientrest.TLSClientConfig{
				Insecure: true,
			},
		}
	}

	if o.Etcd != nil {
		if err := o.Etcd.ApplyTo(&serverConfig.Config); err != nil {
			return err
		}
	}

	serverConfig.Authorization.Authorizer = s.authorizer

	// Get the list of groups the server will support
	builders := s.builders

	// Install schemas
	for _, b := range builders {
		if err := b.InstallSchema(Scheme); err != nil {
			return err
		}
	}

	// Add OpenAPI specs for each group+version
	defsGetter := getOpenAPIDefinitions(builders)
	serverConfig.OpenAPIConfig = genericapiserver.DefaultOpenAPIConfig(
		openapi.GetOpenAPIDefinitionsWithoutDisabledFeatures(defsGetter),
		openapinamer.NewDefinitionNamer(Scheme, scheme.Scheme))

	serverConfig.OpenAPIV3Config = genericapiserver.DefaultOpenAPIV3Config(
		openapi.GetOpenAPIDefinitionsWithoutDisabledFeatures(defsGetter),
		openapinamer.NewDefinitionNamer(Scheme, scheme.Scheme))

	// Add the custom routes to service discovery
	serverConfig.OpenAPIV3Config.PostProcessSpec3 = getOpenAPIPostProcessor(builders)

	serverConfig.SkipOpenAPIInstallation = false
	serverConfig.BuildHandlerChainFunc = func(delegateHandler http.Handler, c *genericapiserver.Config) http.Handler {
		// Call DefaultBuildHandlerChain on the main entrypoint http.Handler
		// See https://github.com/kubernetes/apiserver/blob/v0.28.0/pkg/server/config.go#L906
		// DefaultBuildHandlerChain provides many things, notably CORS, HSTS, cache-control, authz and latency tracking
		requestHandler, err := getAPIHandler(
			delegateHandler,
			c.LoopbackClientConfig,
			builders)
		if err != nil {
			panic(fmt.Sprintf("could not build handler chain func: %s", err.Error()))
		}
		return genericapiserver.DefaultBuildHandlerChain(requestHandler, c)
	}

	// Create the server
	server, err := serverConfig.Complete().New("grafana-apiserver", genericapiserver.NewEmptyDelegate())
	if err != nil {
		return err
	}

	// Install the API Group+version
	for _, b := range builders {
		g, err := b.GetAPIGroupInfo(Scheme, Codecs, serverConfig.RESTOptionsGetter)
		if err != nil {
			return err
		}
		err = server.InstallAPIGroup(g)
		if err != nil {
			return err
		}
	}

	s.restConfig = server.LoopbackClientConfig

	// only write kubeconfig in dev mode
	if s.config.devMode {
		if err := s.ensureKubeConfig(); err != nil {
			return err
		}
	}

	// TODO: this is a hack. see note in ProvideService
	s.handler = func(c *contextmodel.ReqContext) {
		req := c.Req
		if req.URL.Path == "" {
			req.URL.Path = "/"
		}

		//TODO: add support for the existing MetricsEndpointBasicAuth config option
		if req.URL.Path == "/apiserver-metrics" {
			req.URL.Path = "/metrics"
		}

		ctx := req.Context()
		signedInUser := appcontext.MustUser(ctx)

		req.Header.Set("X-Remote-User", strconv.FormatInt(signedInUser.UserID, 10))
		req.Header.Set("X-Remote-Group", "grafana")

		resp := responsewriter.WrapForHTTP1Or2(c.Resp)
		server.Handler.ServeHTTP(resp, req)
	}

	// skip starting the server in prod mode
	if !s.config.devMode {
		return nil
	}

	prepared := server.PrepareRun()
	go func() {
		s.stoppedCh <- prepared.Run(s.stopCh)
	}()
	return nil
}

func (s *service) running(ctx context.Context) error {
	// skip waiting for the server in prod mode
	if !s.config.devMode {
		<-ctx.Done()
		return nil
	}

	select {
	case err := <-s.stoppedCh:
		if err != nil {
			return err
		}
	case <-ctx.Done():
		close(s.stopCh)
	}
	return nil
}

func (s *service) ensureKubeConfig() error {
	clusters := make(map[string]*clientcmdapi.Cluster)
	clusters["default-cluster"] = &clientcmdapi.Cluster{
		Server:                s.restConfig.Host,
		InsecureSkipTLSVerify: true,
	}

	contexts := make(map[string]*clientcmdapi.Context)
	contexts["default-context"] = &clientcmdapi.Context{
		Cluster:   "default-cluster",
		Namespace: "default",
		AuthInfo:  "default",
	}

	authinfos := make(map[string]*clientcmdapi.AuthInfo)
	authinfos["default"] = &clientcmdapi.AuthInfo{
		Token: s.restConfig.BearerToken,
	}

	clientConfig := clientcmdapi.Config{
		Kind:           "Config",
		APIVersion:     "v1",
		Clusters:       clusters,
		Contexts:       contexts,
		CurrentContext: "default-context",
		AuthInfos:      authinfos,
	}

	return clientcmd.WriteToFile(clientConfig, path.Join(s.config.dataPath, "grafana.kubeconfig"))
}
