import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { checkRequestAccessAllowed } from '../util/checkRequestAccessAllowed'

import { isPackagesEnabled } from './flags'
import { PermissionsSyncJobsTable } from './permissions-center/PermissionsSyncJobsTable'
import { SiteAdminAreaRoute } from './SiteAdminArea'

const AnalyticsOverviewPage = lazyComponent(() => import('./analytics/AnalyticsOverviewPage'), 'AnalyticsOverviewPage')
const AnalyticsSearchPage = lazyComponent(() => import('./analytics/AnalyticsSearchPage'), 'AnalyticsSearchPage')
const AnalyticsCodeIntelPage = lazyComponent(
    () => import('./analytics/AnalyticsCodeIntelPage'),
    'AnalyticsCodeIntelPage'
)
const AnalyticsExtensionsPage = lazyComponent(
    () => import('./analytics/AnalyticsExtensionsPage'),
    'AnalyticsExtensionsPage'
)
const AnalyticsUsersPage = lazyComponent(() => import('./analytics/AnalyticsUsersPage'), 'AnalyticsUsersPage')
const AnalyticsCodeInsightsPage = lazyComponent(
    () => import('./analytics/AnalyticsCodeInsightsPage'),
    'AnalyticsCodeInsightsPage'
)
const AnalyticsBatchChangesPage = lazyComponent(
    () => import('./analytics/AnalyticsBatchChangesPage'),
    'AnalyticsBatchChangesPage'
)
const AnalyticsNotebooksPage = lazyComponent(
    () => import('./analytics/AnalyticsNotebooksPage'),
    'AnalyticsNotebooksPage'
)
const SiteAdminConfigurationPage = lazyComponent(
    () => import('./SiteAdminConfigurationPage'),
    'SiteAdminConfigurationPage'
)
const SiteAdminSettingsPage = lazyComponent(() => import('./SiteAdminSettingsPage'), 'SiteAdminSettingsPage')
const SiteAdminExternalServicesArea = lazyComponent(
    () => import('./SiteAdminExternalServicesArea'),
    'SiteAdminExternalServicesArea'
)
const SiteAdminGitHubAppsArea = lazyComponent(() => import('./SiteAdminGitHubAppsArea'), 'SiteAdminGitHubAppsArea')
const SiteAdminRepositoriesPage = lazyComponent(
    () => import('./SiteAdminRepositoriesPage'),
    'SiteAdminRepositoriesPage'
)
const SiteAdminOrgsPage = lazyComponent(() => import('./SiteAdminOrgsPage'), 'SiteAdminOrgsPage')
export const UsersManagement = lazyComponent(() => import('./UserManagement'), 'UsersManagement')
const AccessRequestsPage = lazyComponent(() => import('./AccessRequestsPage'), 'AccessRequestsPage')

const SiteAdminCreateUserPage = lazyComponent(() => import('./SiteAdminCreateUserPage'), 'SiteAdminCreateUserPage')
const SiteAdminTokensPage = lazyComponent(() => import('./SiteAdminTokensPage'), 'SiteAdminTokensPage')
const SiteAdminUpdatesPage = lazyComponent(() => import('./SiteAdminUpdatesPage'), 'SiteAdminUpdatesPage')
const SiteAdminPingsPage = lazyComponent(() => import('./SiteAdminPingsPage'), 'SiteAdminPingsPage')
const SiteAdminReportBugPage = lazyComponent(() => import('./SiteAdminReportBugPage'), 'SiteAdminReportBugPage')
const SiteAdminSurveyResponsesPage = lazyComponent(
    () => import('./SiteAdminSurveyResponsesPage'),
    'SiteAdminSurveyResponsesPage'
)
const SiteAdminMigrationsPage = lazyComponent(() => import('./SiteAdminMigrationsPage'), 'SiteAdminMigrationsPage')
const SiteAdminOutboundRequestsPage = lazyComponent(
    () => import('./SiteAdminOutboundRequestsPage'),
    'SiteAdminOutboundRequestsPage'
)
const SiteAdminBackgroundJobsPage = lazyComponent(
    () => import('./SiteAdminBackgroundJobsPage'),
    'SiteAdminBackgroundJobsPage'
)
const SiteAdminFeatureFlagsPage = lazyComponent(
    () => import('./SiteAdminFeatureFlagsPage'),
    'SiteAdminFeatureFlagsPage'
)
const SiteAdminFeatureFlagConfigurationPage = lazyComponent(
    () => import('./SiteAdminFeatureFlagConfigurationPage'),
    'SiteAdminFeatureFlagConfigurationPage'
)
const OutboundWebhooksPage = lazyComponent(
    () => import('./outbound-webhooks/OutboundWebhooksPage'),
    'OutboundWebhooksPage'
)
const OutgoingWebhookCreatePage = lazyComponent(() => import('./outbound-webhooks/CreatePage'), 'CreatePage')
const OutgoingWebhookEditPage = lazyComponent(() => import('./outbound-webhooks/EditPage'), 'EditPage')
const SiteAdminWebhooksPage = lazyComponent(() => import('./SiteAdminWebhooksPage'), 'SiteAdminWebhooksPage')
const SiteAdminWebhookCreatePage = lazyComponent(
    () => import('./SiteAdminWebhookCreatePage'),
    'SiteAdminWebhookCreatePage'
)
const SiteAdminWebhookPage = lazyComponent(() => import('./SiteAdminWebhookPage'), 'SiteAdminWebhookPage')
const SiteAdminSlowRequestsPage = lazyComponent(
    () => import('./SiteAdminSlowRequestsPage'),
    'SiteAdminSlowRequestsPage'
)
const SiteAdminWebhookUpdatePage = lazyComponent(
    () => import('./SiteAdminWebhookUpdatePage'),
    'SiteAdminWebhookUpdatePage'
)
const SiteAdminPackagesPage = lazyComponent(() => import('./SiteAdminPackagesPage'), 'SiteAdminPackagesPage')

export const otherSiteAdminRoutes: readonly SiteAdminAreaRoute[] = [
    {
        path: '/',
        render: () => <AnalyticsOverviewPage />,
    },
    {
        path: '/analytics/search',
        render: () => <AnalyticsSearchPage />,
    },
    {
        path: '/analytics/code-intel',
        render: () => <AnalyticsCodeIntelPage />,
    },
    {
        path: '/analytics/extensions',
        render: () => <AnalyticsExtensionsPage />,
    },
    {
        path: '/analytics/users',
        render: () => <AnalyticsUsersPage />,
    },
    {
        path: '/analytics/code-insights',
        render: () => <AnalyticsCodeInsightsPage />,
    },
    {
        path: '/analytics/batch-changes',
        render: () => <AnalyticsBatchChangesPage />,
    },
    {
        path: '/analytics/notebooks',
        render: () => <AnalyticsNotebooksPage />,
    },
    {
        path: '/configuration',
        render: props => <SiteAdminConfigurationPage {...props} />,
    },
    {
        path: '/global-settings',
        render: props => <SiteAdminSettingsPage {...props} />,
    },
    {
        path: '/github-apps/*',
        render: props => <SiteAdminGitHubAppsArea {...props} />,
    },
    {
        path: '/external-services/*',
        render: props => <SiteAdminExternalServicesArea {...props} />,
    },
    {
        path: '/repositories',
        render: props => <SiteAdminRepositoriesPage {...props} />,
    },
    {
        path: '/organizations',
        render: props => <SiteAdminOrgsPage {...props} />,
    },
    {
        path: '/account-requests',
        render: () => <AccessRequestsPage />,
        condition: () => checkRequestAccessAllowed(window.context),
    },
    {
        path: '/users/new',
        render: () => <SiteAdminCreateUserPage />,
    },
    {
        path: '/tokens',
        render: props => <SiteAdminTokensPage {...props} />,
    },
    {
        path: '/updates',
        render: props => <SiteAdminUpdatesPage {...props} />,
    },
    {
        path: '/pings',
        render: props => <SiteAdminPingsPage {...props} />,
    },
    {
        path: '/report-bug',
        render: props => <SiteAdminReportBugPage {...props} />,
    },
    {
        path: '/surveys',
        render: props => <SiteAdminSurveyResponsesPage {...props} />,
    },
    {
        path: '/migrations',
        render: props => <SiteAdminMigrationsPage {...props} />,
    },
    {
        path: '/outbound-requests',
        render: props => <SiteAdminOutboundRequestsPage {...props} />,
    },
    {
        path: '/background-jobs',
        render: props => <SiteAdminBackgroundJobsPage {...props} />,
    },
    {
        path: '/feature-flags',
        render: props => <SiteAdminFeatureFlagsPage {...props} />,
    },
    {
        path: '/feature-flags/configuration/:name',
        render: props => <SiteAdminFeatureFlagConfigurationPage {...props} />,
    },
    {
        path: '/webhooks/outgoing',
        render: props => <OutboundWebhooksPage {...props} />,
    },
    {
        path: '/webhooks/outgoing/create',
        render: props => <OutgoingWebhookCreatePage {...props} />,
    },
    {
        path: '/webhooks/outgoing/:id',
        render: props => <OutgoingWebhookEditPage {...props} />,
    },
    {
        path: '/webhooks/incoming',
        render: props => <SiteAdminWebhooksPage {...props} />,
    },
    {
        path: '/webhooks/incoming/create',
        render: props => <SiteAdminWebhookCreatePage {...props} />,
    },
    {
        path: '/webhooks/incoming/:id',
        render: props => <SiteAdminWebhookPage {...props} />,
    },
    {
        path: '/webhooks/incoming/:id/edit',
        render: props => <SiteAdminWebhookUpdatePage {...props} />,
    },
    {
        path: '/slow-requests',
        render: props => <SiteAdminSlowRequestsPage {...props} />,
    },
    {
        path: '/packages',
        render: props => <SiteAdminPackagesPage {...props} />,
        condition: isPackagesEnabled,
    },
    {
        path: '/permissions-syncs',
        render: props => <PermissionsSyncJobsTable {...props} />,
    },
]

const siteAdminUserManagementRoute: SiteAdminAreaRoute = {
    path: '/users',
    render: () => <UsersManagement isEnterprise={false} renderAssignmentModal={() => null} />,
}

export const siteAdminAreaRoutes: readonly SiteAdminAreaRoute[] = [
    ...otherSiteAdminRoutes,
    siteAdminUserManagementRoute,
]
