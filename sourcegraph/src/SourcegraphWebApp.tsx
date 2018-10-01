import { Notifications } from '@sourcegraph/extensions-client-common/lib/app/notifications/Notifications'
import { createController as createExtensionsController } from '@sourcegraph/extensions-client-common/lib/client/controller'
import { ConfiguredExtension } from '@sourcegraph/extensions-client-common/lib/extensions/extension'
import { ClientConnection, connectAsPage } from '@sourcegraph/extensions-client-common/lib/messaging'
import {
    ConfigurationCascadeOrError,
    ConfigurationSubject,
    ConfiguredSubject,
    Settings,
} from '@sourcegraph/extensions-client-common/lib/settings'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import ServerIcon from 'mdi-react/ServerIcon'
import * as React from 'react'
import { Route } from 'react-router'
import { BrowserRouter } from 'react-router-dom'
import { combineLatest, from, Subscription } from 'rxjs'
import { startWith } from 'rxjs/operators'
import { EMPTY_ENVIRONMENT as EXTENSIONS_EMPTY_ENVIRONMENT } from 'sourcegraph/module/client/environment'
import { TextDocumentItem } from 'sourcegraph/module/client/types/textDocument'
import { currentUser } from './auth'
import * as GQL from './backend/graphqlschema'
import { FeedbackText } from './components/FeedbackText'
import { HeroPage } from './components/HeroPage'
import { Tooltip } from './components/tooltip/Tooltip'
import { ExtensionsEnvironmentProps } from './extensions/environment/ExtensionsEnvironment'
import { ExtensionAreaRoute } from './extensions/extension/ExtensionArea'
import { ExtensionAreaHeaderNavItem } from './extensions/extension/ExtensionAreaHeader'
import { ExtensionsAreaRoute } from './extensions/ExtensionsArea'
import { ExtensionsAreaHeaderActionButton } from './extensions/ExtensionsAreaHeader'
import { createExtensionsContextController } from './extensions/ExtensionsClientCommonContext'
import {
    ConfigurationCascadeProps,
    createMessageTransports,
    ExtensionsControllerProps,
    ExtensionsProps,
} from './extensions/ExtensionsClientCommonContext'
import { Layout, LayoutProps } from './Layout'
import { updateUserSessionStores } from './marketing/util'
import { RepoHeaderActionButton } from './repo/RepoHeader'
import { RepoRevContainerRoute } from './repo/RepoRevContainer'
import { clientConfiguration } from './settings/configuration'
import { SiteAdminAreaRoute } from './site-admin/SiteAdminArea'
import { SiteAdminSideBarGroups } from './site-admin/SiteAdminSidebar'
import { eventLogger } from './tracking/eventLogger'
import { UserAccountAreaRoute } from './user/account/UserAccountArea'
import { UserAccountSidebarItems } from './user/account/UserAccountSidebar'
import { UserAreaRoute } from './user/area/UserArea'
import { UserAreaHeaderNavItem } from './user/area/UserAreaHeader'
import { isErrorLike } from './util/errors'

export interface SourcegraphWebAppProps {
    extensionAreaRoutes: ReadonlyArray<ExtensionAreaRoute>
    extensionAreaHeaderNavItems: ReadonlyArray<ExtensionAreaHeaderNavItem>
    extensionsAreaRoutes: ReadonlyArray<ExtensionsAreaRoute>
    extensionsAreaHeaderActionButtons: ReadonlyArray<ExtensionsAreaHeaderActionButton>
    siteAdminAreaRoutes: ReadonlyArray<SiteAdminAreaRoute>
    siteAdminSideBarGroups: SiteAdminSideBarGroups
    siteAdminOverviewComponents: ReadonlyArray<React.ComponentType>
    userAreaHeaderNavItems: ReadonlyArray<UserAreaHeaderNavItem>
    userAreaRoutes: ReadonlyArray<UserAreaRoute>
    userAccountSideBarItems: UserAccountSidebarItems
    userAccountAreaRoutes: ReadonlyArray<UserAccountAreaRoute>
    repoRevContainerRoutes: ReadonlyArray<RepoRevContainerRoute>
    repoHeaderActionButtons: ReadonlyArray<RepoHeaderActionButton>
}

interface SourcegraphWebAppState
    extends ConfigurationCascadeProps,
        ExtensionsProps,
        ExtensionsEnvironmentProps,
        ExtensionsControllerProps {
    error?: Error
    user?: GQL.IUser | null

    viewerSubject: LayoutProps['viewerSubject']

    /**
     * Whether the light theme is enabled or not
     */
    isLightTheme: boolean

    /**
     * The current search query in the navbar.
     */
    navbarSearchQuery: string

    /** Whether the help popover is shown. */
    showHelpPopover: boolean

    clientConnection: Promise<ClientConnection>
}

const LIGHT_THEME_LOCAL_STORAGE_KEY = 'light-theme'

/** A fallback configuration subject that can be constructed synchronously at initialization time. */
const SITE_SUBJECT_NO_ADMIN: Pick<GQL.IConfigurationSubject, 'id' | 'viewerCanAdminister'> = {
    id: window.context.siteGQLID,
    viewerCanAdminister: false,
}

/**
 * The root component
 */
export class SourcegraphWebApp extends React.Component<SourcegraphWebAppProps, SourcegraphWebAppState> {
    constructor(props: SourcegraphWebAppProps) {
        super(props)
        const clientConnection = connectAsPage()
        const extensions = createExtensionsContextController(clientConnection)
        this.state = {
            isLightTheme: localStorage.getItem(LIGHT_THEME_LOCAL_STORAGE_KEY) !== 'false',
            navbarSearchQuery: '',
            showHelpPopover: false,
            configurationCascade: { subjects: null, merged: null },
            extensions,
            extensionsEnvironment: EXTENSIONS_EMPTY_ENVIRONMENT,
            extensionsController: createExtensionsController(extensions.context, createMessageTransports),
            viewerSubject: SITE_SUBJECT_NO_ADMIN,
            clientConnection,
        }
    }

    private subscriptions = new Subscription()

    public componentDidMount(): void {
        updateUserSessionStores()

        document.body.classList.add('theme')
        this.subscriptions.add(
            currentUser.subscribe(user => this.setState({ user }), () => this.setState({ user: null }))
        )

        this.state.clientConnection
            .then(connection => {
                connection
                    .getSettings()
                    .then(settings => clientConfiguration.next(settings))
                    .catch(error => console.error(error))

                connection.onSettings(settings => clientConfiguration.next(settings))
            })
            .catch(error => console.error(error))

        this.subscriptions.add(
            combineLatest(
                from(this.state.extensions.context.configurationCascade).pipe(startWith(null)),
                currentUser.pipe(startWith(null)),
                clientConfiguration
            ).subscribe(([cascade, user, clientConfiguration]) => {
                this.setState(() => {
                    if (clientConfiguration !== undefined) {
                        return {
                            viewerSubject: {
                                id: 'Client',
                                viewerCanAdminister: true,
                            },
                        }
                    } else if (user) {
                        return { viewerSubject: user }
                    } else if (
                        cascade &&
                        !isErrorLike(cascade) &&
                        cascade.subjects &&
                        !isErrorLike(cascade.subjects) &&
                        cascade.subjects.length > 0
                    ) {
                        return { viewerSubject: cascade.subjects[0].subject }
                    } else {
                        return { viewerSubject: SITE_SUBJECT_NO_ADMIN }
                    }
                })
            })
        )

        this.subscriptions.add(this.state.extensionsController)

        this.subscriptions.add(
            this.state.extensions.context.configurationCascade.subscribe(
                v => this.onConfigurationCascadeChange(v),
                err => console.error(err)
            )
        )

        // Keep the Sourcegraph extensions controller's extensions up-to-date.
        //
        // TODO(sqs): handle loading and errors
        this.subscriptions.add(
            this.state.extensions.viewerConfiguredExtensions.subscribe(
                extensions => this.onViewerConfiguredExtensionsChange(extensions),
                err => console.error(err)
            )
        )
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
        document.body.classList.remove('theme')
        document.body.classList.remove('theme-light')
        document.body.classList.remove('theme-dark')
    }

    public componentDidUpdate(): void {
        localStorage.setItem(LIGHT_THEME_LOCAL_STORAGE_KEY, this.state.isLightTheme + '')
        document.body.classList.toggle('theme-light', this.state.isLightTheme)
        document.body.classList.toggle('theme-dark', !this.state.isLightTheme)
    }

    public render(): React.ReactFragment | null {
        if (this.state.error) {
            return <HeroPage icon={AlertCircleIcon} title={'Something happened'} subtitle={this.state.error.message} />
        }

        if (window.pageError && window.pageError.statusCode !== 404) {
            const statusCode = window.pageError.statusCode
            const statusText = window.pageError.statusText
            const errorMessage = window.pageError.error
            const errorID = window.pageError.errorID

            let subtitle: JSX.Element | undefined
            if (errorID) {
                subtitle = <FeedbackText headerText="Sorry, there's been a problem." />
            }
            if (errorMessage) {
                subtitle = (
                    <div className="app__error">
                        {subtitle}
                        {subtitle && <hr />}
                        <pre>{errorMessage}</pre>
                    </div>
                )
            } else {
                subtitle = <div className="app__error">{subtitle}</div>
            }
            return <HeroPage icon={ServerIcon} title={`${statusCode}: ${statusText}`} subtitle={subtitle} />
        }

        const { user } = this.state
        if (user === undefined) {
            return null
        }

        const { children, ...props } = this.props

        return (
            <>
                <BrowserRouter key={0}>
                    <Route
                        path="/"
                        // tslint:disable-next-line:jsx-no-lambda RouteProps.render is an exception
                        render={routeComponentProps => (
                            <Layout
                                {...props}
                                {...routeComponentProps}
                                user={user}
                                viewerSubject={this.state.viewerSubject}
                                configurationCascade={this.state.configurationCascade}
                                // Theme
                                isLightTheme={this.state.isLightTheme}
                                onThemeChange={this.onThemeChange}
                                // Search query
                                navbarSearchQuery={this.state.navbarSearchQuery}
                                onNavbarQueryChange={this.onNavbarQueryChange}
                                // Help popover
                                showHelpPopover={this.state.showHelpPopover}
                                onHelpPopoverToggle={this.onHelpPopoverToggle}
                                // Extensions
                                extensions={this.state.extensions}
                                extensionsEnvironment={this.state.extensionsEnvironment}
                                extensionsOnVisibleTextDocumentsChange={this.extensionsOnVisibleTextDocumentsChange}
                                extensionsController={this.state.extensionsController}
                                clientConnection={this.state.clientConnection}
                            />
                        )}
                    />
                </BrowserRouter>
                <Tooltip key={1} />
                <Notifications key={2} extensionsController={this.state.extensionsController} />
            </>
        )
    }

    private onThemeChange = () => {
        this.setState(
            state => ({ isLightTheme: !state.isLightTheme }),
            () => {
                eventLogger.log(this.state.isLightTheme ? 'LightThemeClicked' : 'DarkThemeClicked')
            }
        )
    }

    private onNavbarQueryChange = (navbarSearchQuery: string) => {
        this.setState({ navbarSearchQuery })
    }

    private onHelpPopoverToggle = (visible?: boolean): void => {
        eventLogger.log('HelpPopoverToggled')
        this.setState(prevState => ({
            // If visible is any non-boolean type (e.g., MouseEvent), treat it as undefined. This lets callers use
            // onHelpPopoverToggle directly in an event handler without wrapping it in an another function.
            showHelpPopover: visible !== true && visible !== false ? !prevState.showHelpPopover : visible,
        }))
    }

    private onConfigurationCascadeChange(
        configurationCascade: ConfigurationCascadeOrError<ConfigurationSubject, Settings>
    ): void {
        this.setState(
            prevState => {
                const update: Pick<SourcegraphWebAppState, 'configurationCascade' | 'extensionsEnvironment'> = {
                    configurationCascade,
                    extensionsEnvironment: prevState.extensionsEnvironment,
                }
                if (
                    configurationCascade.subjects !== null &&
                    !isErrorLike(configurationCascade.subjects) &&
                    configurationCascade.merged !== null &&
                    !isErrorLike(configurationCascade.merged)
                ) {
                    // Only update Sourcegraph extensions environment configuration if the configuration was
                    // successfully parsed.
                    //
                    // TODO(sqs): Think through how this error should be handled.
                    update.extensionsEnvironment = {
                        ...prevState.extensionsEnvironment,
                        configuration: {
                            subjects: configurationCascade.subjects.filter(
                                (subject): subject is ConfiguredSubject<ConfigurationSubject, Settings> =>
                                    subject.settings !== null && !isErrorLike(subject.settings)
                            ),
                            merged: configurationCascade.merged,
                        },
                    }
                }
                return update
            },
            () => this.state.extensionsController.setEnvironment(this.state.extensionsEnvironment)
        )
    }

    private onViewerConfiguredExtensionsChange(viewerConfiguredExtensions: ConfiguredExtension[]): void {
        this.setState(
            prevState => ({
                extensionsEnvironment: {
                    ...prevState.extensionsEnvironment,
                    extensions: viewerConfiguredExtensions,
                },
            }),
            () => this.state.extensionsController.setEnvironment(this.state.extensionsEnvironment)
        )
    }

    private extensionsOnVisibleTextDocumentsChange = (visibleTextDocuments: TextDocumentItem[] | null): void => {
        this.setState(
            prevState => ({ extensionsEnvironment: { ...prevState.extensionsEnvironment, visibleTextDocuments } }),
            () => this.state.extensionsController.setEnvironment(this.state.extensionsEnvironment)
        )
    }
}
