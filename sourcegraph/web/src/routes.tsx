import * as React from 'react'
import { Redirect, RouteComponentProps } from 'react-router'
import { LayoutProps } from './Layout'
import { parseSearchURLQuery } from './search'
import { lazyComponent } from './util/lazyComponent'

const SearchPage = lazyComponent(() => import('./search/input/SearchPage'), 'SearchPage')
const SearchResults = lazyComponent(() => import('./search/results/SearchResults'), 'SearchResults')
const SiteAdminArea = lazyComponent(() => import('./site-admin/SiteAdminArea'), 'SiteAdminArea')
const ExtensionsArea = lazyComponent(() => import('./extensions/ExtensionsArea'), 'ExtensionsArea')

export interface LayoutRouteComponentProps extends RouteComponentProps<any>, LayoutProps {}

export interface LayoutRouteProps {
    path: string
    exact?: boolean
    render: (props: LayoutRouteComponentProps) => React.ReactNode

    /**
     * Whether or not to force the width of the page to be narrow.
     */
    forceNarrowWidth?: boolean
}

/**
 * Holds properties for repository+ routes.
 */
export const repoRevRoute: LayoutRouteProps = {
    path: '/:repoRevAndRest+',
    render: lazyComponent(() => import('./repo/RepoContainer'), 'RepoContainer'),
}

/**
 * Holds all top-level routes for the app because both the navbar and the main content area need to
 * switch over matched path.
 *
 * See https://reacttraining.com/react-router/web/example/sidebar
 */
export const routes: ReadonlyArray<LayoutRouteProps> = [
    {
        path: '/',
        render: (props: any) =>
            window.context.sourcegraphDotComMode && !props.user ? (
                <Redirect to="/welcome" />
            ) : (
                <Redirect to="/search" />
            ),
        exact: true,
    },
    {
        path: '/search',
        render: (props: any) =>
            parseSearchURLQuery(props.location.search) ? (
                <SearchResults {...props} deployType={window.context.deployType} />
            ) : (
                <SearchPage {...props} />
            ),
        exact: true,
    },
    {
        path: '/search/searches',
        render: lazyComponent(
            () => import('./search/saved-searches/RedirectToUserSavedSearches'),
            'RedirectToUserSavedSearches'
        ),
        exact: true,
        forceNarrowWidth: true,
    },
    {
        path: '/open',
        render: lazyComponent(() => import('./open/OpenPage'), 'OpenPage'),
        exact: true,
        forceNarrowWidth: true,
    },
    {
        path: '/sign-in',
        render: lazyComponent(() => import('./auth/SignInPage'), 'SignInPage'),
        exact: true,
        forceNarrowWidth: true,
    },
    {
        path: '/sign-up',
        render: lazyComponent(
            async () => ({ SignUpPage: (await import('./auth/SignUpPage')).SignUpPage }),
            'SignUpPage'
        ),
        exact: true,
        forceNarrowWidth: true,
    },
    {
        path: '/settings',
        render: lazyComponent(() => import('./user/settings/RedirectToUserSettings'), 'RedirectToUserSettings'),
    },
    {
        path: '/user',
        render: lazyComponent(() => import('./user/settings/RedirectToUserPage'), 'RedirectToUserPage'),
    },
    {
        path: '/organizations',
        render: lazyComponent(() => import('./org/OrgsArea'), 'OrgsArea'),
    },
    {
        path: '/search',
        render: props => <SearchResults {...props} deployType={window.context.deployType} />,
        exact: true,
    },
    {
        path: '/site-admin/init',
        exact: true,
        render: lazyComponent(() => import('./site-admin/SiteInitPage'), 'SiteInitPage'),
        forceNarrowWidth: false,
    },
    {
        path: '/site-admin',
        render: props => (
            <SiteAdminArea
                {...props}
                routes={props.siteAdminAreaRoutes}
                sideBarGroups={props.siteAdminSideBarGroups}
                overviewComponents={props.siteAdminOverviewComponents}
            />
        ),
    },
    {
        path: '/password-reset',
        render: lazyComponent(() => import('./auth/ResetPasswordPage'), 'ResetPasswordPage'),
        exact: true,
        forceNarrowWidth: true,
    },
    {
        path: '/explore',
        render: lazyComponent(() => import('./explore/ExploreArea'), 'ExploreArea'),
        exact: true,
    },
    {
        path: '/discussions',
        render: lazyComponent(() => import('./discussions/DiscussionsPage'), 'DiscussionsPage'),
        exact: true,
    },
    {
        path: '/search/scope/:id',
        render: lazyComponent(() => import('./search/input/ScopePage'), 'ScopePage'),
        exact: true,
    },
    {
        path: '/api/console',
        render: lazyComponent(() => import('./api/APIConsole'), 'APIConsole'),
        exact: true,
    },
    {
        path: '/users/:username',
        render: lazyComponent(() => import('./user/area/UserArea'), 'UserArea'),
    },
    {
        path: '/survey/:score?',
        render: lazyComponent(() => import('./marketing/SurveyPage'), 'SurveyPage'),
    },
    {
        path: '/extensions',
        render: props => <ExtensionsArea {...props} routes={props.extensionsAreaRoutes} />,
    },
    {
        path: '/help',
        render: () => {
            // Force a hard reload so that we delegate to the HTTP handler for /help, which handles
            // redirecting /help to https://docs.sourcegraph.com. That logic is not duplicated in
            // the web app because that would add complexity with no user benefit.
            //
            // TODO(sqs): This currently has a bug in dev mode where you can't go back to the app
            // after following the redirect. This will be fixed when we run docsite on
            // http://localhost:5080 in Procfile because then the redirect will be cross-domain and
            // won't reuse the same history stack.
            window.location.reload()
            return null
        },
    },
    {
        path: '/snippets',
        render: lazyComponent(() => import('./snippets/SnippetsPage'), 'SnippetsPage'),
    },
    repoRevRoute,
]
