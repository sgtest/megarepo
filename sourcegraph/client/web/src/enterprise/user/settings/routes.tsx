import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { userSettingsAreaRoutes } from '../../../user/settings/routes'
import { UserSettingsAreaRoute } from '../../../user/settings/UserSettingsArea'
import { SHOW_BUSINESS_FEATURES } from '../../dotcom/productSubscriptions/features'
import type { ExecutorsUserAreaProps } from '../../executors/ExecutorsUserArea'

import { UserEventLogsPageProps } from './UserEventLogsPage'

const ExecutorsUserArea = lazyComponent<ExecutorsUserAreaProps, 'ExecutorsUserArea'>(
    () => import('../../executors/ExecutorsUserArea'),
    'ExecutorsUserArea'
)

export const enterpriseUserSettingsAreaRoutes: readonly UserSettingsAreaRoute[] = [
    ...userSettingsAreaRoutes,
    {
        path: 'permissions',
        render: lazyComponent(() => import('./auth/UserSettingsPermissionsPage'), 'UserSettingsPermissionsPage'),
    },
    {
        path: 'event-log',
        render: lazyComponent<UserEventLogsPageProps, 'UserEventLogsPage'>(
            () => import('./UserEventLogsPage'),
            'UserEventLogsPage'
        ),
    },
    {
        path: 'executors/*',
        render: props => <ExecutorsUserArea {...props} namespaceID={props.user.id} />,
        condition: ({ user: { viewerCanAdminister } }) => viewerCanAdminister,
    },
    {
        path: 'batch-changes',
        render: lazyComponent(
            () => import('../../batches/settings/BatchChangesSettingsArea'),
            'BatchChangesSettingsArea'
        ),
        condition: ({ batchChangesEnabled, user: { viewerCanAdminister } }) =>
            batchChangesEnabled && viewerCanAdminister,
    },
    {
        path: 'subscriptions/:subscriptionUUID',
        render: lazyComponent(
            () => import('../productSubscriptions/UserSubscriptionsProductSubscriptionPage'),
            'UserSubscriptionsProductSubscriptionPage'
        ),
        condition: () => SHOW_BUSINESS_FEATURES,
    },
    {
        path: 'subscriptions',
        render: lazyComponent(
            () => import('../productSubscriptions/UserSubscriptionsProductSubscriptionsPage'),
            'UserSubscriptionsProductSubscriptionsPage'
        ),
        condition: () => SHOW_BUSINESS_FEATURES,
    },
]
