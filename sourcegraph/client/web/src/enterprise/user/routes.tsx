import { Redirect, RouteComponentProps } from 'react-router'

import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { userAreaRoutes } from '../../user/area/routes'
import { UserAreaRoute, UserAreaRouteContext } from '../../user/area/UserArea'
import { CreateBatchChangePageProps } from '../batches/create/CreateBatchChangePage'
import { CreateOrEditBatchChangePageProps } from '../batches/create/CreateOrEditBatchChangePage'
import { ExecutionAreaProps, NamespaceBatchChangesAreaProps } from '../batches/global/GlobalBatchChangesArea'
import { SHOW_BUSINESS_FEATURES } from '../dotcom/productSubscriptions/features'
import { enterpriseNamespaceAreaRoutes } from '../namespaces/routes'

const NamespaceBatchChangesArea = lazyComponent<NamespaceBatchChangesAreaProps, 'NamespaceBatchChangesArea'>(
    () => import('../batches/global/GlobalBatchChangesArea'),
    'NamespaceBatchChangesArea'
)

const ExecutionArea = lazyComponent<ExecutionAreaProps, 'ExecutionArea'>(
    () => import('../batches/global/GlobalBatchChangesArea'),
    'ExecutionArea'
)

const CreateOrEditBatchChangePage = lazyComponent<CreateOrEditBatchChangePageProps, 'CreateOrEditBatchChangePage'>(
    () => import('../batches/create/CreateOrEditBatchChangePage'),
    'CreateOrEditBatchChangePage'
)

const CreateBatchChangePage = lazyComponent<CreateBatchChangePageProps, 'CreateBatchChangePage'>(
    () => import('../batches/create/CreateBatchChangePage'),
    'CreateBatchChangePage'
)

export const enterpriseUserAreaRoutes: readonly UserAreaRoute[] = [
    ...userAreaRoutes,
    ...enterpriseNamespaceAreaRoutes,

    // Redirect from previous /users/:username/subscriptions -> /users/:username/settings/subscriptions.
    {
        path: '/subscriptions/:page*',
        render: (props: UserAreaRouteContext & RouteComponentProps<{ page: string }>) => (
            <Redirect
                to={`${props.url}/settings/subscriptions${
                    props.match.params.page ? `/${props.match.params.page}` : ''
                }`}
            />
        ),
        condition: () => SHOW_BUSINESS_FEATURES,
    },
    {
        path: '/batch-changes/create',
        render: props => <CreateBatchChangePage headingElement="h1" {...props} initialNamespaceID={props.user.id} />,
        condition: ({ batchChangesEnabled }) => batchChangesEnabled,
        fullPage: true,
    },
    {
        path: '/batch-changes/:batchChangeName/edit',
        render: ({ match, ...props }: UserAreaRouteContext & RouteComponentProps<{ batchChangeName: string }>) => (
            <CreateOrEditBatchChangePage
                {...props}
                initialNamespaceID={props.user.id}
                batchChangeName={match.params.batchChangeName}
            />
        ),
        condition: ({ batchChangesEnabled, batchChangesExecutionEnabled }) =>
            batchChangesEnabled && batchChangesExecutionEnabled,
        fullPage: true,
    },
    {
        path: '/batch-changes/:batchChangeName/executions/:batchSpecID/configuration',
        render: ({ match, ...props }: UserAreaRouteContext & RouteComponentProps<{ batchChangeName: string }>) => (
            <CreateOrEditBatchChangePage
                {...props}
                initialNamespaceID={props.user.id}
                batchChangeName={match.params.batchChangeName}
                isReadOnly={true}
            />
        ),
        condition: ({ batchChangesEnabled, batchChangesExecutionEnabled }) =>
            batchChangesEnabled && batchChangesExecutionEnabled,
        fullPage: true,
    },
    {
        path: '/batch-changes/:batchChangeName/executions/:batchSpecID',
        render: (props: UserAreaRouteContext & RouteComponentProps<{ batchSpecID: string }>) => (
            <ExecutionArea {...props} namespaceID={props.user.id} />
        ),
        condition: ({ batchChangesEnabled, batchChangesExecutionEnabled }) =>
            batchChangesEnabled && batchChangesExecutionEnabled,
        fullPage: true,
    },
    {
        path: '/batch-changes',
        render: props => <NamespaceBatchChangesArea {...props} namespaceID={props.user.id} />,
        condition: ({ batchChangesEnabled }) => batchChangesEnabled,
    },
]
