import { Navigate, RouteObject } from 'react-router-dom'

import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { LegacyRoute } from '../LegacyRouteContext'
import { routes } from '../routes'
import { EnterprisePageRoutes } from '../routes.constants'

import { isSentinelEnabled } from './sentinel/utils/isSentinelEnabled'

const GlobalNotebooksArea = lazyComponent(() => import('../notebooks/GlobalNotebooksArea'), 'GlobalNotebooksArea')
const GlobalBatchChangesArea = lazyComponent(
    () => import('./batches/global/GlobalBatchChangesArea'),
    'GlobalBatchChangesArea'
)
const GlobalCodeMonitoringArea = lazyComponent(
    () => import('./code-monitoring/global/GlobalCodeMonitoringArea'),
    'GlobalCodeMonitoringArea'
)
const CodeInsightsRouter = lazyComponent(() => import('./insights/CodeInsightsRouter'), 'CodeInsightsRouter')
const SearchContextsListPage = lazyComponent(
    () => import('./searchContexts/SearchContextsListPage'),
    'SearchContextsListPage'
)
const SentinelRouter = lazyComponent(() => import('./sentinel/SentinelRouter'), 'SentinelRouter')
const CreateSearchContextPage = lazyComponent(
    () => import('./searchContexts/CreateSearchContextPage'),
    'CreateSearchContextPage'
)
const EditSearchContextPage = lazyComponent(
    () => import('./searchContexts/EditSearchContextPage'),
    'EditSearchContextPage'
)
const SearchContextPage = lazyComponent(() => import('./searchContexts/SearchContextPage'), 'SearchContextPage')
const CodySearchPage = lazyComponent(() => import('../cody/search/CodySearchPage'), 'CodySearchPage')
const CodyChatPage = lazyComponent(() => import('../cody/chat/CodyChatPage'), 'CodyChatPage')
const OwnPage = lazyComponent(() => import('./own/OwnPage'), 'OwnPage')
const AppAuthCallbackPage = lazyComponent(() => import('./app/AppAuthCallbackPage'), 'AppAuthCallbackPage')
const AppSetup = lazyComponent(() => import('./app/setup/AppSetupWizard'), 'AppSetupWizard')

export const enterpriseRoutes: RouteObject[] = [
    {
        path: `${EnterprisePageRoutes.AppSetup}/*`,
        handle: { isFullPage: true },
        element: (
            <LegacyRoute
                render={props => <AppSetup telemetryService={props.telemetryService} />}
                condition={({ isSourcegraphApp }) => isSourcegraphApp}
            />
        ),
    },
    {
        path: EnterprisePageRoutes.BatchChanges,
        element: (
            <LegacyRoute
                render={props => <GlobalBatchChangesArea {...props} />}
                // We also render this route on sourcegraph.com as a precaution in case anyone
                // follows an in-app link to /batch-changes from sourcegraph.com; the component
                // will just redirect the visitor to the marketing page
                condition={({ batchChangesEnabled, isSourcegraphDotCom }) => batchChangesEnabled || isSourcegraphDotCom}
            />
        ),
    },
    {
        path: EnterprisePageRoutes.CodeMonitoring,
        element: <LegacyRoute render={props => <GlobalCodeMonitoringArea {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.Insights,
        element: (
            <LegacyRoute
                render={props => <CodeInsightsRouter {...props} />}
                condition={({ codeInsightsEnabled }) => !!codeInsightsEnabled}
            />
        ),
    },
    {
        path: EnterprisePageRoutes.Sentinel,
        element: (
            <LegacyRoute
                render={props => <SentinelRouter {...props} />}
                condition={props => isSentinelEnabled(props)}
            />
        ),
    },
    {
        path: EnterprisePageRoutes.Contexts,
        element: <LegacyRoute render={props => <SearchContextsListPage {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.CreateContext,
        element: <LegacyRoute render={props => <CreateSearchContextPage {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.EditContext,
        element: <LegacyRoute render={props => <EditSearchContextPage {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.Context,
        element: <LegacyRoute render={props => <SearchContextPage {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.SearchNotebook,
        element: <Navigate to={EnterprisePageRoutes.Notebooks} replace={true} />,
    },
    {
        path: EnterprisePageRoutes.Notebooks + '/*',
        element: <LegacyRoute render={props => <GlobalNotebooksArea {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.CodySearch,
        element: <LegacyRoute render={props => <CodySearchPage {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.Cody + '/*',
        element: <LegacyRoute render={props => <CodyChatPage {...props} />} />,
    },
    {
        path: EnterprisePageRoutes.Own,
        element: <OwnPage />,
    },
    {
        path: EnterprisePageRoutes.AppAuthCallback,
        element: (
            <LegacyRoute
                render={() => <AppAuthCallbackPage />}
                condition={({ isSourcegraphApp }) => isSourcegraphApp}
            />
        ),
    },
    ...routes,
]
