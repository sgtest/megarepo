import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { upperFirst } from 'lodash'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import MapSearchIcon from 'mdi-react/MapSearchIcon'
import * as React from 'react'
import { Route, RouteComponentProps, Switch } from 'react-router'
import { combineLatest, merge, Observable, of, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, map, mapTo, startWith, switchMap } from 'rxjs/operators'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import { gql } from '../../../../shared/src/graphql/graphql'
import * as GQL from '../../../../shared/src/graphql/schema'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { SettingsCascadeProps } from '../../../../shared/src/settings/settings'
import { createAggregateError, ErrorLike, isErrorLike } from '../../../../shared/src/util/errors'
import { queryGraphQL } from '../../backend/graphql'
import { ErrorBoundary } from '../../components/ErrorBoundary'
import { HeroPage } from '../../components/HeroPage'
import { NamespaceProps } from '../../namespaces'
import { ThemeProps } from '../../theme'
import { RouteDescriptor } from '../../util/contributions'
import { OrgAreaHeaderNavItem, OrgHeader } from './OrgHeader'
import { OrgInvitationPage } from './OrgInvitationPage'

function queryOrganization(args: { name: string }): Observable<GQL.IOrg | null> {
    return queryGraphQL(
        gql`
            query Organization($name: String!) {
                organization(name: $name) {
                    __typename
                    id
                    name
                    displayName
                    url
                    settingsURL
                    viewerPendingInvitation {
                        id
                        sender {
                            username
                            displayName
                            avatarURL
                            createdAt
                        }
                        respondURL
                    }
                    viewerIsMember
                    viewerCanAdminister
                    createdAt
                }
            }
        `,
        args
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.organization) {
                throw createAggregateError(errors)
            }
            return data.organization
        })
    )
}

const NotFoundPage = () => (
    <HeroPage icon={MapSearchIcon} title="404: Not Found" subtitle="Sorry, the requested organization was not found." />
)

export interface OrgAreaRoute extends RouteDescriptor<OrgAreaPageProps> {}

interface Props
    extends RouteComponentProps<{ name: string }>,
        PlatformContextProps,
        SettingsCascadeProps,
        ThemeProps,
        ExtensionsControllerProps {
    orgAreaRoutes: ReadonlyArray<OrgAreaRoute>
    orgAreaHeaderNavItems: ReadonlyArray<OrgAreaHeaderNavItem>

    /**
     * The currently authenticated user.
     */
    authenticatedUser: GQL.IUser | null
}

interface State {
    /**
     * The fetched org or an error if an error occurred; undefined while loading.
     */
    orgOrError?: GQL.IOrg | ErrorLike
}

/**
 * Properties passed to all page components in the org area.
 */
export interface OrgAreaPageProps
    extends ExtensionsControllerProps,
        PlatformContextProps,
        SettingsCascadeProps,
        ThemeProps,
        NamespaceProps {
    /** The org that is the subject of the page. */
    org: GQL.IOrg

    /** Called when the organization is updated and must be reloaded. */
    onOrganizationUpdate: () => void

    /** The currently authenticated user. */
    authenticatedUser: GQL.IUser | null
}

/**
 * An organization's public profile area.
 */
export class OrgArea extends React.Component<Props> {
    public state: State = {}

    private routeMatchChanges = new Subject<{ name: string }>()
    private refreshRequests = new Subject<void>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        // Changes to the route-matched org name.
        const nameChanges = this.routeMatchChanges.pipe(
            map(({ name }) => name),
            distinctUntilChanged()
        )

        // Fetch organization.
        this.subscriptions.add(
            combineLatest(nameChanges, merge(this.refreshRequests.pipe(mapTo(false)), of(true)))
                .pipe(
                    switchMap(([name, forceRefresh]) => {
                        type PartialStateUpdate = Pick<State, 'orgOrError'>
                        return queryOrganization({ name }).pipe(
                            catchError(error => [error]),
                            map((c): PartialStateUpdate => ({ orgOrError: c })),

                            // Don't clear old org data while we reload, to avoid unmounting all components during
                            // loading.
                            startWith<PartialStateUpdate>(forceRefresh ? { orgOrError: undefined } : {})
                        )
                    })
                )
                .subscribe(stateUpdate => this.setState(stateUpdate), err => console.error(err))
        )

        this.routeMatchChanges.next(this.props.match.params)
    }

    public componentWillReceiveProps(props: Props): void {
        if (props.match.params !== this.props.match.params) {
            this.routeMatchChanges.next(props.match.params)
        }
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        if (!this.state.orgOrError) {
            return null // loading
        }
        if (isErrorLike(this.state.orgOrError)) {
            return (
                <HeroPage icon={AlertCircleIcon} title="Error" subtitle={upperFirst(this.state.orgOrError.message)} />
            )
        }

        const context: OrgAreaPageProps = {
            authenticatedUser: this.props.authenticatedUser,
            org: this.state.orgOrError,
            onOrganizationUpdate: this.onDidUpdateOrganization,
            extensionsController: this.props.extensionsController,
            platformContext: this.props.platformContext,
            settingsCascade: this.props.settingsCascade,
            isLightTheme: this.props.isLightTheme,
            namespace: this.state.orgOrError,
        }

        if (this.props.location.pathname === `${this.props.match.url}/invitation`) {
            // The OrgInvitationPage is displayed without the OrgHeader because it is modal-like.
            return <OrgInvitationPage {...context} onDidRespondToInvitation={this.onDidRespondToInvitation} />
        }

        return (
            <div className="org-area w-100">
                <OrgHeader
                    {...this.props}
                    {...context}
                    navItems={this.props.orgAreaHeaderNavItems}
                    className="border-bottom mt-4"
                />
                <div className="container mt-3">
                    <ErrorBoundary location={this.props.location}>
                        <React.Suspense fallback={<LoadingSpinner className="icon-inline m-2" />}>
                            <Switch>
                                {this.props.orgAreaRoutes.map(
                                    ({ path, exact, render, condition = () => true }) =>
                                        condition(context) && (
                                            <Route
                                                path={this.props.match.url + path}
                                                key="hardcoded-key" // see https://github.com/ReactTraining/react-router/issues/4578#issuecomment-334489490
                                                exact={exact}
                                                // tslint:disable-next-line:jsx-no-lambda
                                                render={routeComponentProps =>
                                                    render({ ...context, ...routeComponentProps })
                                                }
                                            />
                                        )
                                )}
                                <Route key="hardcoded-key" component={NotFoundPage} />
                            </Switch>
                        </React.Suspense>
                    </ErrorBoundary>
                </div>
            </div>
        )
    }

    private onDidRespondToInvitation = () => this.refreshRequests.next()

    private onDidUpdateOrganization = () => this.refreshRequests.next()
}
