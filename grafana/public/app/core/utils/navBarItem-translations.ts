import { t } from 'app/core/internationalization';

// Maps the ID of the nav item to a translated phrase to later pass to <Trans />
// Because the navigation content is dynamic (defined in the backend), we can not use
// the normal inline message definition method.

// see pkg/api/index.go
export function getNavTitle(navId: string | undefined) {
  // the switch cases must match the ID of the navigation item, as defined in the backend nav model
  switch (navId) {
    case 'home':
      return t('nav.home.title', 'Home');
    case 'new':
      return t('nav.new.title', 'New');
    case 'create':
      return t('nav.create.title', 'Create');
    case 'create-dashboard':
      return t('nav.create-dashboard.title', 'Dashboard');
    case 'folder':
      return t('nav.create-folder.title', 'Folder');
    case 'import':
      return t('nav.create-import.title', 'Import dashboard');
    case 'alert':
      return t('nav.create-alert.title', 'Create alert rule');
    case 'starred':
      return t('nav.starred.title', 'Starred');
    case 'starred-empty':
      return t('nav.starred-empty.title', 'Your starred dashboards will appear here');
    case 'dashboards':
      return t('nav.dashboards.title', 'Dashboards');
    case 'dashboards/browse':
      return t('nav.dashboards.title', 'Dashboards');
    case 'dashboards/playlists':
      return t('nav.playlists.title', 'Playlists');
    case 'dashboards/snapshots':
      return t('nav.snapshots.title', 'Snapshots');
    case 'dashboards/library-panels':
      return t('nav.library-panels.title', 'Library panels');
    case 'reports':
      return t('nav.reporting.title', 'Reporting');
    case 'dashboards/public':
      return t('nav.public.title', 'Public dashboards');
    case 'dashboards/new':
      return t('nav.new-dashboard.title', 'New dashboard');
    case 'dashboards/folder/new':
      return t('nav.new-folder.title', 'New folder');
    case 'dashboards/import':
      return t('nav.create-import.title', 'Import dashboard');
    case 'scenes':
      return t('nav.scenes.title', 'Scenes');
    case 'explore':
      return t('nav.explore.title', 'Explore');
    case 'alerting':
      return t('nav.alerting.title', 'Alerting');
    case 'plugin-page-grafana-oncall-app':
      return t('nav.oncall.title', 'OnCall');
    case 'alerting-legacy':
      return t('nav.alerting-legacy.title', 'Alerting (legacy)');
    case 'alert-home':
      return t('nav.alerting-home.title', 'Home');
    case 'alert-list':
      return t('nav.alerting-list.title', 'Alert rules');
    case 'receivers':
      return t('nav.alerting-receivers.title', 'Contact points');
    case 'am-routes':
      return t('nav.alerting-am-routes.title', 'Notification policies');
    case 'channels':
      return t('nav.alerting-channels.title', 'Notification channels');
    case 'silences':
      return t('nav.alerting-silences.title', 'Silences');
    case 'groups':
      return t('nav.alerting-groups.title', 'Groups');
    case 'alerting-admin':
      return t('nav.alerting-admin.title', 'Admin');
    case 'cfg':
      return t('nav.config.title', 'Administration');
    case 'cfg/general':
      return t('nav.config-general.title', 'General');
    case 'cfg/plugins':
      return t('nav.config-plugins.title', 'Plugins and data');
    case 'cfg/access':
      return t('nav.config-access.title', 'Users and access');
    case 'datasources':
      return t('nav.datasources.title', 'Data sources');
    case 'authentication':
      return t('nav.authentication.title', 'Authentication');
    case 'licensing':
      return t('nav.statistics-and-licensing.title', 'Statistics and licensing');
    case 'recordedQueries':
      return t('nav.recorded-queries.title', 'Recorded queries');
    case 'correlations':
      return t('nav.correlations.title', 'Correlations');
    case 'users':
      return t('nav.users.title', 'Users');
    case 'teams':
      return t('nav.teams.title', 'Teams');
    case 'plugins':
      return t('nav.plugins.title', 'Plugins');
    case 'org-settings':
      return t('nav.org-settings.title', 'Default preferences');
    case 'apikeys':
      return t('nav.api-keys.title', 'API keys');
    case 'serviceaccounts':
      return t('nav.service-accounts.title', 'Service accounts');
    case 'admin':
      return t('nav.admin.title', 'Server admin');
    case 'support-bundles':
      return t('nav.support-bundles.title', 'Support bundles');
    case 'global-users':
      return t('nav.global-users.title', 'Users');
    case 'global-orgs':
      return t('nav.global-orgs.title', 'Organizations');
    case 'server-settings':
      return t('nav.server-settings.title', 'Settings');
    case 'storage':
      return t('nav.storage.title', 'Storage');
    case 'upgrading':
      return t('nav.upgrading.title', 'Stats and license');
    case 'monitoring':
      return t('nav.monitoring.title', 'Observability');
    case 'apps':
      return t('nav.apps.title', 'Apps');
    case 'alerts-and-incidents':
      return t('nav.alerts-and-incidents.title', 'Alerts & IRM');
    case 'plugin-page-grafana-incident-app':
      return t('nav.incidents.title', 'Incidents');
    case 'plugin-page-grafana-ml-app':
      return t('nav.machine-learning.title', 'Machine learning');
    case 'plugin-page-grafana-slo-app':
      return t('nav.slo.title', 'SLO');
    case 'plugin-page-k6-app':
      return t('nav.performance-testing.title', 'Performance testing');
    case 'monitoring':
      return t('nav.observability.title', 'Observability');
    case 'plugin-page-grafana-k8s-app':
      return t('nav.kubernetes.title', 'Kubernetes');
    case 'plugin-page-grafana-app-observability-app':
      return t('nav.application.title', 'Application (preview)');
    case 'plugin-page-grafana-pyroscope-app':
      return t('nav.profiles.title', 'Profiles');
    case 'plugin-page-grafana-kowalski-app':
      return t('nav.frontend.title', 'Frontend');
    case 'plugin-page-grafana-synthetic-monitoring-app':
      return t('nav.synthetics.title', 'Synthetics');
    case 'help':
      return t('nav.help.title', 'Help');
    case 'profile/settings':
      return t('nav.profile/settings.title', 'Profile');
    case 'profile/notifications':
      return t('nav.profile/notifications.title', 'Notification history');
    case 'profile/password':
      return t('nav.profile/password.title', 'Change password');
    case 'sign-out':
      return t('nav.sign-out.title', 'Sign out');
    case 'search':
      return t('nav.search-dashboards.title', 'Search dashboards');
    case 'connections':
      return t('nav.connections.title', 'Connections');
    case 'connections-add-new-connection':
      return t('nav.add-new-connections.title', 'Add new connection');
    case 'connections-datasources':
      return t('nav.data-sources.title', 'Data sources');
    case 'standalone-plugin-page-/connections/infrastructure':
      return t('nav.infrastructure.title', 'Integrations');
    case 'standalone-plugin-page-/connections/connect-data':
      return t('nav.connect-data.title', 'Connect data');
    case 'plugin-page-grafana-detect-app':
      return t('nav.detect.title', 'Detect');
    case 'plugin-page-grafana-quaderno-app':
      return t('nav.grafana-quaderno.title', 'Grafana Quaderno');
    default:
      return undefined;
  }
}

export function getNavSubTitle(navId: string | undefined) {
  switch (navId) {
    case 'dashboards':
      return t('nav.dashboards.subtitle', 'Create and manage dashboards to visualize your data');
    case 'dashboards/browse':
      return t('nav.dashboards.subtitle', 'Create and manage dashboards to visualize your data');
    case 'manage-folder':
      return t('nav.manage-folder.subtitle', 'Manage folder dashboards and permissions');
    case 'dashboards/playlists':
      return t('nav.playlists.subtitle', 'Groups of dashboards that are displayed in a sequence');
    case 'dashboards/snapshots':
      return t(
        'nav.snapshots.subtitle',
        'Interactive, publically available, point-in-time representations of dashboards'
      );
    case 'dashboards/library-panels':
      return t('nav.library-panels.subtitle', 'Reusable panels that can be added to multiple dashboards');
    case 'alerting':
      return t('nav.alerting.subtitle', 'Learn about problems in your systems moments after they occur');
    case 'alert-list':
      return t('nav.alerting-list.subtitle', 'Rules that determine whether an alert will fire');
    case 'receivers':
      return t(
        'nav.alerting-receivers.subtitle',
        'Choose how to notify your contact points when an alert instance fires'
      );
    case 'am-routes':
      return t('nav.alerting-am-routes.subtitle', 'Determine how alerts are routed to contact points');
    case 'silences':
      return t('nav.alerting-silences.subtitle', 'Stop notifications from one or more alerting rules');
    case 'groups':
      return t('nav.alerting-groups.subtitle', 'See grouped alerts from an Alertmanager instance');
    case 'datasources':
      return t('nav.datasources.subtitle', 'Add and configure data sources');
    case 'correlations':
      return t('nav.correlations.subtitle', 'Add and configure correlations');
    case 'users':
      return t('nav.users.subtitle', 'Invite and assign roles to users');
    case 'teams':
      return t('nav.teams.subtitle', 'Groups of users that have common dashboard and permission needs');
    case 'plugins':
      return t('nav.plugins.subtitle', 'Extend the Grafana experience with plugins');
    case 'org-settings':
      return t('nav.org-settings.subtitle', 'Manage preferences across an organization');
    case 'apikeys':
      return t('nav.api-keys.subtitle', 'Manage and create API keys that are used to interact with Grafana HTTP APIs');
    case 'serviceaccounts':
      return t('nav.service-accounts.subtitle', 'Use service accounts to run automated workloads in Grafana');
    case 'global-users':
      return t('nav.global-users.subtitle', 'Manage users in Grafana');
    case 'global-orgs':
      return t('nav.global-orgs.subtitle', 'Isolated instances of Grafana running on the same server');
    case 'server-settings':
      return t('nav.server-settings.subtitle', 'View the settings defined in your Grafana config');
    case 'storage':
      return t('nav.storage.subtitle', 'Manage file storage');
    case 'support-bundles':
      return t('nav.support-bundles.subtitle', 'Download support bundles');
    case 'admin':
      return t(
        'nav.admin.subtitle',
        'Manage server-wide settings and access to resources such as organizations, users, and licenses'
      );
    case 'cfg/general':
      return t('nav.config-general.subtitle', 'Manage default preferences and settings across Grafana');
    case 'cfg/plugins':
      return t('nav.config-plugins.subtitle', 'Install plugins and define the relationships between data');
    case 'cfg/access':
      return t('nav.config-access.subtitle', 'Configure access for individual users, teams, and service accounts');
    case 'apps':
      return t('nav.apps.subtitle', 'App plugins that extend the Grafana experience');
    case 'monitoring':
      return t('nav.monitoring.subtitle', 'Monitoring and infrastructure apps');
    case 'alerts-and-incidents':
      return t('nav.alerts-and-incidents.subtitle', 'Alerting and incident management apps');
    case 'connections-add-new-connection':
      return t('nav.connections.subtitle', 'Browse and create new connections');
    case 'connections-datasources':
      return t('nav.data-sources.subtitle', 'View and manage your connected data source connections');
    default:
      return undefined;
  }
}
