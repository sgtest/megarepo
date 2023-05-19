import 'focus-visible'

import * as React from 'react'

import { ApolloProvider } from '@apollo/client'
import { RouterProvider, createBrowserRouter, createRoutesFromElements, Route } from 'react-router-dom'
import { combineLatest, from, Subscription, fromEvent, Observable } from 'rxjs'

import { logger } from '@sourcegraph/common'
import { GraphQLClient, HTTPStatusError } from '@sourcegraph/http-client'
import { SharedSpanName, TraceSpanProvider } from '@sourcegraph/observability-client'
import { FetchFileParameters, fetchHighlightedFileLineRanges } from '@sourcegraph/shared/src/backend/file'
import { setCodeIntelSearchContext } from '@sourcegraph/shared/src/codeintel/searchContext'
import { Controller as ExtensionsController } from '@sourcegraph/shared/src/extensions/controller'
import { createNoopController } from '@sourcegraph/shared/src/extensions/createNoopLoadedController'
import { PlatformContext } from '@sourcegraph/shared/src/platform/context'
import { ShortcutProvider } from '@sourcegraph/shared/src/react-shortcuts'
import {
    getUserSearchContextNamespaces,
    fetchSearchContexts,
    fetchSearchContext,
    fetchSearchContextBySpec,
    createSearchContext,
    updateSearchContext,
    deleteSearchContext,
    isSearchContextSpecAvailable,
    SearchQueryStateStoreProvider,
    getDefaultSearchContextSpec,
} from '@sourcegraph/shared/src/search'
import { FilterType } from '@sourcegraph/shared/src/search/query/filters'
import { filterExists } from '@sourcegraph/shared/src/search/query/validate'
import { aggregateStreamingSearch } from '@sourcegraph/shared/src/search/stream'
import {
    EMPTY_SETTINGS_CASCADE,
    SettingsCascadeProps,
    SettingsProvider,
} from '@sourcegraph/shared/src/settings/settings'
import { TemporarySettingsProvider } from '@sourcegraph/shared/src/settings/temporary/TemporarySettingsProvider'
import { TemporarySettingsStorage } from '@sourcegraph/shared/src/settings/temporary/TemporarySettingsStorage'
import { WildcardThemeContext, WildcardTheme } from '@sourcegraph/wildcard'

import { authenticatedUser as authenticatedUserSubject, AuthenticatedUser, authenticatedUserValue } from './auth'
import { getWebGraphQLClient } from './backend/graphql'
import { isBatchChangesExecutionEnabled } from './batches'
import { ComponentsComposer } from './components/ComponentsComposer'
import { ErrorBoundary } from './components/ErrorBoundary'
import { FeatureFlagsLocalOverrideAgent } from './featureFlags/FeatureFlagsProvider'
import { LegacyLayout, LegacyLayoutProps } from './LegacyLayout'
import { LegacyRouteContextProvider } from './LegacyRouteContext'
import { PageError } from './PageError'
import { createPlatformContext } from './platform/context'
import { parseSearchURL } from './search'
import { SearchResultsCacheProvider } from './search/results/SearchResultsCacheProvider'
import { GLOBAL_SEARCH_CONTEXT_SPEC } from './SearchQueryStateObserver'
import { StaticAppConfig } from './staticAppConfig'
import { setQueryStateFromSettings, useNavbarQueryState } from './stores'
import { eventLogger } from './tracking/eventLogger'
import { UserSessionStores } from './UserSessionStores'
import { siteSubjectNoAdmin, viewerSubjectFromSettings } from './util/settings'

interface LegacySourcegraphWebAppState extends SettingsCascadeProps {
    error?: Error

    /**
     * The currently authenticated user:
     * - `AuthenticatedUser` if the viewer is authenticated.
     * - `null` if the viewer is anonymous.
     */
    authenticatedUser: AuthenticatedUser | null

    /** GraphQL client initialized asynchronously to restore persisted cache. */
    graphqlClient?: GraphQLClient

    temporarySettingsStorage?: TemporarySettingsStorage

    viewerSubject: LegacyLayoutProps['viewerSubject']

    selectedSearchContextSpec?: string
}

const WILDCARD_THEME: WildcardTheme = {
    isBranded: true,
}

/**
 * The root component.
 */
export class LegacySourcegraphWebApp extends React.Component<StaticAppConfig, LegacySourcegraphWebAppState> {
    private readonly subscriptions = new Subscription()
    private readonly platformContext: PlatformContext = createPlatformContext()
    private readonly extensionsController: ExtensionsController | null = createNoopController(this.platformContext)

    constructor(props: StaticAppConfig) {
        super(props)

        if (this.extensionsController !== null) {
            this.subscriptions.add(this.extensionsController)
        }

        this.state = {
            authenticatedUser: authenticatedUserValue,
            settingsCascade: EMPTY_SETTINGS_CASCADE,
            viewerSubject: siteSubjectNoAdmin(),
        }
    }

    public componentDidMount(): void {
        const parsedSearchURL = parseSearchURL(window.location.search)
        const parsedSearchQuery = parsedSearchURL.query || ''

        document.documentElement.classList.add('theme')

        getWebGraphQLClient()
            .then(graphqlClient => {
                this.setState({
                    graphqlClient,
                    temporarySettingsStorage: new TemporarySettingsStorage(
                        graphqlClient,
                        window.context.isAuthenticatedUser
                    ),
                })
            })
            .catch(error => {
                logger.error('Error initializing GraphQL client', error)
            })

        this.subscriptions.add(
            combineLatest([
                from(this.platformContext.settings),
                // Start with `undefined` while we don't know if the viewer is authenticated or not.
                authenticatedUserSubject,
            ]).subscribe(([settingsCascade, authenticatedUser]) => {
                setQueryStateFromSettings(settingsCascade)
                this.setState({
                    settingsCascade,
                    authenticatedUser,
                    viewerSubject: viewerSubjectFromSettings(settingsCascade, authenticatedUser),
                })
            })
        )

        /**
         * Listens for uncaught 401 errors when a user when a user was previously authenticated.
         *
         * Don't subscribe to this event when there wasn't an authenticated user,
         * as it could lead to an infinite loop of 401 -> reload -> 401
         */
        if (window.context.isAuthenticatedUser) {
            this.subscriptions.add(
                fromEvent<ErrorEvent>(window, 'error').subscribe(event => {
                    if (event?.error instanceof HTTPStatusError && event.error.status === 401) {
                        location.reload()
                    }
                })
            )
        }

        if (parsedSearchQuery && !filterExists(parsedSearchQuery, FilterType.context)) {
            // If a context filter does not exist in the query, we have to switch the selected context
            // to global to match the UI with the backend semantics (if no context is specified in the query,
            // the query is run in global context).
            this.setSelectedSearchContextSpecWithNoChecks(GLOBAL_SEARCH_CONTEXT_SPEC)
        }
        if (!parsedSearchQuery) {
            // If no query is present (e.g. search page, settings page),
            // select the user's default search context.
            this.setSelectedSearchContextSpecToDefault()
        }

        this.setWorkspaceSearchContext(this.state.selectedSearchContextSpec).catch(error => {
            logger.error('Error sending search context to extensions!', error)
        })
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): React.ReactNode {
        const pageError = window.pageError
        if (pageError && pageError.statusCode !== 404) {
            return <PageError pageError={pageError} />
        }

        const { authenticatedUser, graphqlClient, temporarySettingsStorage } = this.state

        if (graphqlClient === undefined || temporarySettingsStorage === undefined) {
            return null
        }

        const legacyContext = {
            ...this.props,
            selectedSearchContextSpec: this.state.selectedSearchContextSpec,
            setSelectedSearchContextSpec: this.setSelectedSearchContextSpec,
            codeIntelligenceEnabled: !!this.props.codeInsightsEnabled,
            notebooksEnabled: this.props.notebooksEnabled,
            codeMonitoringEnabled: this.props.codeMonitoringEnabled,
            searchAggregationEnabled: this.props.searchAggregationEnabled,
            platformContext: this.platformContext,
            authenticatedUser,
            viewerSubject: this.state.viewerSubject,
            settingsCascade: this.state.settingsCascade,
            extensionsController: this.extensionsController,
        }

        const router = createBrowserRouter(
            createRoutesFromElements(
                <Route
                    path="*"
                    element={
                        <LegacyLayout
                            {...legacyContext}
                            {...this.props}
                            batchChangesExecutionEnabled={isBatchChangesExecutionEnabled(this.state.settingsCascade)}
                            batchChangesWebhookLogsEnabled={window.context.batchChangesWebhookLogsEnabled}
                            fetchHighlightedFileLineRanges={this.fetchHighlightedFileLineRanges}
                            telemetryService={eventLogger}
                            isSourcegraphDotCom={window.context.sourcegraphDotComMode}
                            isSourcegraphApp={window.context.sourcegraphAppMode}
                            isSearchContextSpecAvailable={isSearchContextSpecAvailable}
                            searchContextsEnabled={this.props.searchContextsEnabled}
                            getUserSearchContextNamespaces={getUserSearchContextNamespaces}
                            fetchSearchContexts={fetchSearchContexts}
                            fetchSearchContextBySpec={fetchSearchContextBySpec}
                            fetchSearchContext={fetchSearchContext}
                            createSearchContext={createSearchContext}
                            updateSearchContext={updateSearchContext}
                            deleteSearchContext={deleteSearchContext}
                            streamSearch={aggregateStreamingSearch}
                        />
                    }
                />
            )
        )

        return (
            <ComponentsComposer
                components={[
                    // `ComponentsComposer` provides children via `React.cloneElement`.
                    /* eslint-disable react/no-children-prop, react/jsx-key */
                    <ApolloProvider client={graphqlClient} children={undefined} />,
                    <WildcardThemeContext.Provider value={WILDCARD_THEME} />,
                    <SettingsProvider settingsCascade={this.state.settingsCascade} />,
                    <ErrorBoundary location={null} />,
                    <TraceSpanProvider name={SharedSpanName.AppMount} />,
                    <FeatureFlagsLocalOverrideAgent />,
                    <ShortcutProvider />,
                    <TemporarySettingsProvider temporarySettingsStorage={temporarySettingsStorage} />,
                    <SearchResultsCacheProvider />,
                    <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState} />,
                    <LegacyRouteContextProvider context={legacyContext} />,
                    /* eslint-enable react/no-children-prop, react/jsx-key */
                ]}
            >
                <RouterProvider router={router} />
                <UserSessionStores />
            </ComponentsComposer>
        )
    }

    private setSelectedSearchContextSpecWithNoChecks = (spec: string): void => {
        this.setState({ selectedSearchContextSpec: spec })
        this.setWorkspaceSearchContext(spec).catch(error => {
            logger.error('Error sending search context to extensions', error)
        })
    }

    private setSelectedSearchContextSpec = (spec: string): void => {
        if (!this.props.searchContextsEnabled) {
            return
        }

        // The global search context is always available.
        if (spec === GLOBAL_SEARCH_CONTEXT_SPEC) {
            this.setSelectedSearchContextSpecWithNoChecks(spec)
        }

        // Check if the wanted search context is available.
        this.subscriptions.add(
            isSearchContextSpecAvailable({
                spec,
                platformContext: this.platformContext,
            }).subscribe(isAvailable => {
                if (isAvailable) {
                    this.setSelectedSearchContextSpecWithNoChecks(spec)
                } else if (!this.state.selectedSearchContextSpec) {
                    // If the wanted search context is not available and
                    // there is no currently selected search context,
                    // set the current selection to the default search context.
                    // Otherwise, keep the current selection.
                    this.setSelectedSearchContextSpecToDefault()
                }
            })
        )
    }

    private setSelectedSearchContextSpecToDefault = (): void => {
        if (!this.props.searchContextsEnabled) {
            return
        }

        this.subscriptions.add(
            getDefaultSearchContextSpec({ platformContext: this.platformContext }).subscribe(spec => {
                // Fall back to global if no default is returned.
                this.setSelectedSearchContextSpecWithNoChecks(spec || GLOBAL_SEARCH_CONTEXT_SPEC)
            })
        )
    }

    private async setWorkspaceSearchContext(spec: string | undefined): Promise<void> {
        // NOTE(2022-09-08) Inform the inlined code from
        // sourcegraph/code-intel-extensions about the change of search context.
        // The old extension code previously accessed this information from the
        // 'sourcegraph' npm package, and updating the context like this was the
        // simplest solution to mirror the old behavior while deprecating
        // extensions on a tight deadline. It would be nice to properly pass
        // around this via React state in the future.
        setCodeIntelSearchContext(spec)
        if (this.extensionsController === null) {
            return
        }
        const extensionHostAPI = await this.extensionsController.extHostAPI
        await extensionHostAPI.setSearchContext(spec)
    }

    private fetchHighlightedFileLineRanges = (
        parameters: FetchFileParameters,
        force?: boolean | undefined
    ): Observable<string[][]> =>
        fetchHighlightedFileLineRanges({ ...parameters, platformContext: this.platformContext }, force)
}
