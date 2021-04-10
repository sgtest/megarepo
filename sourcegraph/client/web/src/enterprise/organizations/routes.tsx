import React from 'react'
import { Redirect } from 'react-router'

import { OrgAreaRoute } from '../../org/area/OrgArea'
import { orgAreaRoutes } from '../../org/area/routes'
import { lazyComponent } from '../../util/lazyComponent'
import { NamespaceBatchChangesAreaProps } from '../batches/global/GlobalBatchChangesArea'
import { enterpriseNamespaceAreaRoutes } from '../namespaces/routes'

const NamespaceBatchChangesArea = lazyComponent<NamespaceBatchChangesAreaProps, 'NamespaceBatchChangesArea'>(
    () => import('../batches/global/GlobalBatchChangesArea'),
    'NamespaceBatchChangesArea'
)

export const enterpriseOrganizationAreaRoutes: readonly OrgAreaRoute[] = [
    ...orgAreaRoutes,
    ...enterpriseNamespaceAreaRoutes,
    {
        path: '/campaigns',
        render: ({ location }) => <Redirect to={location.pathname.replace('/campaigns', '/batch-changes')} />,
    },
    {
        path: '/batch-changes',
        render: props => <NamespaceBatchChangesArea {...props} namespaceID={props.org.id} />,
        condition: props => !props.isSourcegraphDotCom && window.context.batchChangesEnabled,
    },
]
