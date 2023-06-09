import { Navigate, RouteObject } from 'react-router-dom'

import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { LegacyRoute } from '../../LegacyRouteContext'
import { EnterprisePageRoutes, PageRoutes } from '../../routes.constants'

const AppSetup = lazyComponent(() => import('./setup/AppSetupWizard'), 'AppSetupWizard')
const AppAuthCallbackPage = lazyComponent(() => import('./AppAuthCallbackPage'), 'AppAuthCallbackPage')
const CodyChatPage = lazyComponent(() => import('../../cody/chat/CodyChatPage'), 'CodyChatPage')
const RedirectToUserPage = lazyComponent(() => import('../../user/settings/RedirectToUserPage'), 'RedirectToUserPage')
const SiteAdminArea = lazyComponent(() => import('../../site-admin/SiteAdminArea'), 'SiteAdminArea')
const ApiConsole = lazyComponent(() => import('../../api/ApiConsole'), 'ApiConsole')
const UserArea = lazyComponent(() => import('../../user/area/UserArea'), 'UserArea')
const RedirectToUserSettings = lazyComponent(
    () => import('../../user/settings/RedirectToUserSettings'),
    'RedirectToUserSettings'
)

export const APP_ROUTES: RouteObject[] = [
    {
        path: PageRoutes.Index,
        // The default page of the Sourcegraph (Cody) app is Cody chat UI page
        element: <Navigate replace={true} to={EnterprisePageRoutes.Cody} />,
    },
    {
        path: PageRoutes.Search,
        // The default page of the Sourcegraph (Cody) app is Cody chat UI page
        element: <Navigate replace={true} to={EnterprisePageRoutes.Cody} />,
    },
    {
        path: EnterprisePageRoutes.Cody + '/*',
        element: <LegacyRoute render={props => <CodyChatPage {...props} />} />,
    },
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
        path: EnterprisePageRoutes.AppAuthCallback,
        element: (
            <LegacyRoute
                render={() => <AppAuthCallbackPage />}
                condition={({ isSourcegraphApp }) => isSourcegraphApp}
            />
        ),
    },
    {
        path: PageRoutes.User,
        element: <LegacyRoute render={props => <RedirectToUserPage {...props} />} />,
    },
    {
        path: PageRoutes.Settings,
        element: <LegacyRoute render={props => <RedirectToUserSettings {...props} />} />,
    },
    {
        path: PageRoutes.SiteAdmin,
        element: (
            <LegacyRoute
                render={props => (
                    <SiteAdminArea
                        {...props}
                        routes={props.siteAdminAreaRoutes}
                        sideBarGroups={props.siteAdminSideBarGroups}
                        overviewComponents={props.siteAdminOverviewComponents}
                    />
                )}
            />
        ),
    },
    {
        path: PageRoutes.ApiConsole,
        element: <ApiConsole />,
    },
    {
        path: PageRoutes.UserArea,
        element: <LegacyRoute render={props => <UserArea {...props} />} />,
    },
]
