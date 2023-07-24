package backgroundsvcs

import (
	"github.com/grafana/grafana/pkg/infra/metrics"
	"github.com/grafana/grafana/pkg/infra/remotecache"
	"github.com/grafana/grafana/pkg/infra/tracing"
	uss "github.com/grafana/grafana/pkg/infra/usagestats/service"
	"github.com/grafana/grafana/pkg/infra/usagestats/statscollector"
	"github.com/grafana/grafana/pkg/plugins/manager/process"
	"github.com/grafana/grafana/pkg/registry"
	"github.com/grafana/grafana/pkg/services/alerting"
	"github.com/grafana/grafana/pkg/services/auth"
	"github.com/grafana/grafana/pkg/services/cleanup"
	"github.com/grafana/grafana/pkg/services/dashboardsnapshots"
	"github.com/grafana/grafana/pkg/services/grpcserver"
	"github.com/grafana/grafana/pkg/services/guardian"
	ldapapi "github.com/grafana/grafana/pkg/services/ldap/api"
	"github.com/grafana/grafana/pkg/services/live"
	"github.com/grafana/grafana/pkg/services/live/pushhttp"
	"github.com/grafana/grafana/pkg/services/login/authinfoservice"
	"github.com/grafana/grafana/pkg/services/loginattempt/loginattemptimpl"
	"github.com/grafana/grafana/pkg/services/ngalert"
	"github.com/grafana/grafana/pkg/services/notifications"
	plugindashboardsservice "github.com/grafana/grafana/pkg/services/plugindashboards/service"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/angulardetectorsprovider"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/keyretriever/dynamic"
	"github.com/grafana/grafana/pkg/services/provisioning"
	publicdashboardsmetric "github.com/grafana/grafana/pkg/services/publicdashboards/metric"
	"github.com/grafana/grafana/pkg/services/rendering"
	"github.com/grafana/grafana/pkg/services/searchV2"
	secretsManager "github.com/grafana/grafana/pkg/services/secrets/manager"
	"github.com/grafana/grafana/pkg/services/serviceaccounts"
	samanager "github.com/grafana/grafana/pkg/services/serviceaccounts/manager"
	"github.com/grafana/grafana/pkg/services/store"
	"github.com/grafana/grafana/pkg/services/store/entity"
	"github.com/grafana/grafana/pkg/services/store/sanitizer"
	"github.com/grafana/grafana/pkg/services/supportbundles/supportbundlesimpl"
	"github.com/grafana/grafana/pkg/services/updatechecker"
)

func ProvideBackgroundServiceRegistry(
	ng *ngalert.AlertNG, cleanup *cleanup.CleanUpService, live *live.GrafanaLive,
	pushGateway *pushhttp.Gateway, notifications *notifications.NotificationService, processManager *process.Manager,
	rendering *rendering.RenderingService, tokenService auth.UserTokenBackgroundService, tracing tracing.Tracer,
	provisioning *provisioning.ProvisioningServiceImpl, alerting *alerting.AlertEngine, usageStats *uss.UsageStats,
	statsCollector *statscollector.Service, grafanaUpdateChecker *updatechecker.GrafanaService,
	pluginsUpdateChecker *updatechecker.PluginsService, metrics *metrics.InternalMetricsService,
	secretsService *secretsManager.SecretsService, remoteCache *remotecache.RemoteCache, StorageService store.StorageService, searchService searchV2.SearchService, entityEventsService store.EntityEventsService,
	saService *samanager.ServiceAccountsService, authInfoService *authinfoservice.Implementation,
	grpcServerProvider grpcserver.Provider, loginAttemptService *loginattemptimpl.Service,
	bundleService *supportbundlesimpl.Service,
	publicDashboardsMetric *publicdashboardsmetric.Service,
	keyRetriever *dynamic.KeyRetriever,
	dynamicAngularDetectorsProvider *angulardetectorsprovider.Dynamic,
	// Need to make sure these are initialized, is there a better place to put them?
	_ dashboardsnapshots.Service, _ *alerting.AlertNotificationService,
	_ serviceaccounts.Service, _ *guardian.Provider,
	_ *plugindashboardsservice.DashboardUpdater, _ *sanitizer.Provider,
	_ *grpcserver.HealthService, _ entity.EntityStoreServer, _ *grpcserver.ReflectionService, _ *ldapapi.Service,
) *BackgroundServiceRegistry {
	return NewBackgroundServiceRegistry(
		ng,
		cleanup,
		live,
		pushGateway,
		notifications,
		rendering,
		tokenService,
		provisioning,
		alerting,
		grafanaUpdateChecker,
		pluginsUpdateChecker,
		metrics,
		usageStats,
		statsCollector,
		tracing,
		remoteCache,
		secretsService,
		StorageService,
		searchService,
		entityEventsService,
		grpcServerProvider,
		saService,
		authInfoService,
		processManager,
		loginAttemptService,
		bundleService,
		publicDashboardsMetric,
		keyRetriever,
		dynamicAngularDetectorsProvider,
	)
}

// BackgroundServiceRegistry provides background services.
type BackgroundServiceRegistry struct {
	services []registry.BackgroundService
}

func NewBackgroundServiceRegistry(s ...registry.BackgroundService) *BackgroundServiceRegistry {
	return &BackgroundServiceRegistry{services: s}
}

func (r *BackgroundServiceRegistry) GetServices() []registry.BackgroundService {
	return r.services
}
