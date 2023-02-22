import React, { Suspense, useCallback, useLayoutEffect, useState } from 'react'

import classNames from 'classnames'
import { Outlet, useLocation, Navigate, useMatches } from 'react-router-dom'
import { Observable } from 'rxjs'

import { TabbedPanelContent } from '@sourcegraph/branded/src/components/panel/TabbedPanelContent'
import { FetchFileParameters } from '@sourcegraph/shared/src/backend/file'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { useKeyboardShortcut } from '@sourcegraph/shared/src/keyboardShortcuts/useKeyboardShortcut'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { Shortcut } from '@sourcegraph/shared/src/react-shortcuts'
import { Settings } from '@sourcegraph/shared/src/schema/settings.schema'
import { SearchContextProps } from '@sourcegraph/shared/src/search'
import { SettingsCascadeProps, SettingsSubjectCommonFields } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { useTheme, Theme } from '@sourcegraph/shared/src/theme'
import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'
import { parseQueryAndHash } from '@sourcegraph/shared/src/util/url'
import { FeedbackPrompt, LoadingSpinner, Panel } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from './auth'
import type { BatchChangesProps } from './batches'
import type { CodeIntelligenceProps } from './codeintel'
import { CodeMonitoringProps } from './codeMonitoring'
import { communitySearchContextsRoutes } from './communitySearchContexts/routes'
import { AppRouterContainer } from './components/AppRouterContainer'
import { ErrorBoundary } from './components/ErrorBoundary'
import { LazyFuzzyFinder } from './components/fuzzyFinder/LazyFuzzyFinder'
import { KeyboardShortcutsHelp } from './components/KeyboardShortcutsHelp/KeyboardShortcutsHelp'
import { useScrollToLocationHash } from './components/useScrollToLocationHash'
import { useUserHistory } from './components/useUserHistory'
import { useFeatureFlag } from './featureFlags/useFeatureFlag'
import { GlobalAlerts } from './global/GlobalAlerts'
import { useHandleSubmitFeedback } from './hooks'
import { SurveyToast } from './marketing/toast'
import { GlobalNavbar } from './nav/GlobalNavbar'
import type { NotebookProps } from './notebooks'
import { EnterprisePageRoutes, PageRoutes } from './routes.constants'
import { parseSearchURLQuery, SearchAggregationProps, SearchStreamingProps } from './search'
import { NotepadContainer } from './search/Notepad'
import { SearchQueryStateObserver } from './SearchQueryStateObserver'
import { useExperimentalFeatures } from './stores'
import { getExperimentalFeatures } from './util/get-experimental-features'
import { parseBrowserRepoURL } from './util/url'

import styles from './Layout.module.scss'

const LazySetupWizard = lazyComponent(() => import('./setup-wizard'), 'SetupWizard')

export interface LegacyLayoutProps
    extends SettingsCascadeProps<Settings>,
        PlatformContextProps,
        ExtensionsControllerProps,
        TelemetryProps,
        SearchContextProps,
        SearchStreamingProps,
        CodeIntelligenceProps,
        BatchChangesProps,
        NotebookProps,
        CodeMonitoringProps,
        SearchAggregationProps {
    authenticatedUser: AuthenticatedUser | null

    /**
     * The subject GraphQL node ID of the viewer, which is used to look up the viewer's settings. This is either
     * the site's GraphQL node ID (for anonymous users) or the authenticated user's GraphQL node ID.
     */
    viewerSubject: SettingsSubjectCommonFields

    // Search
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>

    globbing: boolean
    isSourcegraphDotCom: boolean
}
/**
 * Syntax highlighting changes for WCAG 2.1 contrast compliance (currently behind feature flag)
 * https://github.com/sourcegraph/sourcegraph/issues/36251
 */
const CONTRAST_COMPLIANT_CLASSNAME = 'theme-contrast-compliant-syntax-highlighting'

export const Layout: React.FC<LegacyLayoutProps> = props => {
    const location = useLocation()

    const routeMatches = useMatches()

    const isRepositoryRelatedPage =
        routeMatches.some(
            routeMatch =>
                routeMatch.handle &&
                typeof routeMatch.handle === 'object' &&
                Object.hasOwn(routeMatch.handle, 'isRepoContainer')
        ) ?? false
    // TODO: Move the search box into a shared layout component that is only used for repo routes
    //       and search routes once we have flattened the router hierarchy.
    const isSearchRelatedPage =
        (isRepositoryRelatedPage || routeMatches.some(routeMatch => routeMatch.pathname.startsWith('/search'))) ?? false
    const isSearchHomepage = location.pathname === '/search' && !parseSearchURLQuery(location.search)
    const isSearchConsolePage = routeMatches.some(routeMatch => routeMatch.pathname.startsWith('/search/console'))
    const isSearchNotebooksPage = routeMatches.some(routeMatch =>
        routeMatch.pathname.startsWith(EnterprisePageRoutes.Notebooks)
    )
    const isSearchNotebookListPage = location.pathname === EnterprisePageRoutes.Notebooks

    const { setupWizard } = useExperimentalFeatures()
    const isSetupWizardPage = setupWizard && location.pathname.startsWith(PageRoutes.SetupWizard)

    // enable fuzzy finder by default unless it's explicitly disabled in settings
    const fuzzyFinder = getExperimentalFeatures(props.settingsCascade.final).fuzzyFinder ?? true
    const [isFuzzyFinderVisible, setFuzzyFinderVisible] = useState(false)
    const userHistory = useUserHistory(isRepositoryRelatedPage)

    const communitySearchContextPaths = communitySearchContextsRoutes.map(route => route.path)
    const isCommunitySearchContextPage = communitySearchContextPaths.includes(location.pathname)

    // TODO add a component layer as the parent of the Layout component rendering "top-level" routes that do not render the navbar,
    // so that Layout can always render the navbar.
    const needsSiteInit = window.context?.needsSiteInit
    const disableFeedbackSurvey = window.context?.disableFeedbackSurvey
    const isSiteInit = location.pathname === PageRoutes.SiteAdminInit
    const isSignInOrUp =
        location.pathname === PageRoutes.SignIn ||
        location.pathname === PageRoutes.SignUp ||
        location.pathname === PageRoutes.PasswordReset ||
        location.pathname === PageRoutes.Welcome

    const [enableContrastCompliantSyntaxHighlighting] = useFeatureFlag('contrast-compliant-syntax-highlighting')

    const { theme } = useTheme()
    const [keyboardShortcutsHelpOpen, setKeyboardShortcutsHelpOpen] = useState(false)
    const [feedbackModalOpen, setFeedbackModalOpen] = useState(false)
    const showHelpShortcut = useKeyboardShortcut('keyboardShortcutsHelp')

    const showKeyboardShortcutsHelp = useCallback(() => setKeyboardShortcutsHelpOpen(true), [])
    const hideKeyboardShortcutsHelp = useCallback(() => setKeyboardShortcutsHelpOpen(false), [])
    const showFeedbackModal = useCallback(() => setFeedbackModalOpen(true), [])

    const { handleSubmitFeedback } = useHandleSubmitFeedback({
        routeMatch:
            routeMatches && routeMatches.length > 0 ? routeMatches[routeMatches.length - 1].pathname : undefined,
    })

    useLayoutEffect(() => {
        const isLightTheme = theme === Theme.Light

        document.documentElement.classList.add('theme')
        document.documentElement.classList.toggle('theme-light', isLightTheme)
        document.documentElement.classList.toggle('theme-dark', !isLightTheme)
    }, [theme])

    useScrollToLocationHash(location)

    // Note: this was a poor UX and is disabled for now, see https://github.com/sourcegraph/sourcegraph/issues/30192
    // const [tosAccepted, setTosAccepted] = useState(true) // Assume TOS has been accepted so that we don't show the TOS modal on initial load
    // useEffect(() => setTosAccepted(!props.authenticatedUser || props.authenticatedUser.tosAccepted), [
    //     props.authenticatedUser,
    // ])
    // const afterTosAccepted = useCallback(() => {
    //     setTosAccepted(true)
    // }, [])

    // Remove trailing slash (which is never valid in any of our URLs).
    if (location.pathname !== '/' && location.pathname.endsWith('/')) {
        return <Navigate replace={true} to={{ ...location, pathname: location.pathname.slice(0, -1) }} />
    }

    if (isSetupWizardPage) {
        return (
            <Suspense
                fallback={
                    <div className="flex flex-1">
                        <LoadingSpinner className="m-2" />
                    </div>
                }
            >
                <LazySetupWizard />
            </Suspense>
        )
    }

    return (
        <div
            className={classNames(
                styles.layout,
                enableContrastCompliantSyntaxHighlighting && CONTRAST_COMPLIANT_CLASSNAME
            )}
        >
            {showHelpShortcut?.keybindings.map((keybinding, index) => (
                <Shortcut key={index} {...keybinding} onMatch={showKeyboardShortcutsHelp} />
            ))}
            <KeyboardShortcutsHelp isOpen={keyboardShortcutsHelpOpen} onDismiss={hideKeyboardShortcutsHelp} />

            {feedbackModalOpen && (
                <FeedbackPrompt
                    onSubmit={handleSubmitFeedback}
                    modal={true}
                    openByDefault={true}
                    authenticatedUser={
                        props.authenticatedUser
                            ? {
                                  username: props.authenticatedUser.username || '',
                                  email: props.authenticatedUser.emails.find(email => email.isPrimary)?.email || '',
                              }
                            : null
                    }
                    onClose={() => setFeedbackModalOpen(false)}
                />
            )}

            <GlobalAlerts
                authenticatedUser={props.authenticatedUser}
                settingsCascade={props.settingsCascade}
                isSourcegraphDotCom={props.isSourcegraphDotCom}
            />
            {!isSiteInit && !isSignInOrUp && !props.isSourcegraphDotCom && !disableFeedbackSurvey && (
                <SurveyToast authenticatedUser={props.authenticatedUser} />
            )}
            {!isSiteInit && !isSignInOrUp && (
                <GlobalNavbar
                    {...props}
                    routes={[]}
                    showSearchBox={
                        isSearchRelatedPage &&
                        !isSearchHomepage &&
                        !isCommunitySearchContextPage &&
                        !isSearchConsolePage &&
                        !isSearchNotebooksPage
                    }
                    setFuzzyFinderIsVisible={setFuzzyFinderVisible}
                    isRepositoryRelatedPage={isRepositoryRelatedPage}
                    showKeyboardShortcutsHelp={showKeyboardShortcutsHelp}
                    showFeedbackModal={showFeedbackModal}
                    enableLegacyExtensions={window.context.enableLegacyExtensions}
                />
            )}
            {needsSiteInit && !isSiteInit && <Navigate replace={true} to="/site-admin/init" />}
            <ErrorBoundary location={location}>
                <Suspense
                    fallback={
                        <div className="flex flex-1">
                            <LoadingSpinner className="m-2" />
                        </div>
                    }
                >
                    <AppRouterContainer>
                        <Outlet />
                    </AppRouterContainer>
                </Suspense>
            </ErrorBoundary>
            {parseQueryAndHash(location.search, location.hash).viewState && location.pathname !== PageRoutes.SignIn && (
                <Panel
                    className={styles.panel}
                    position="bottom"
                    defaultSize={350}
                    storageKey="panel-size"
                    ariaLabel="References panel"
                    id="references-panel"
                >
                    <TabbedPanelContent
                        {...props}
                        repoName={`git://${parseBrowserRepoURL(location.pathname).repoName}`}
                        fetchHighlightedFileLineRanges={props.fetchHighlightedFileLineRanges}
                    />
                </Panel>
            )}
            {(isSearchNotebookListPage || (isSearchRelatedPage && !isSearchHomepage)) && (
                <NotepadContainer userId={props.authenticatedUser?.id} />
            )}
            {fuzzyFinder && (
                <LazyFuzzyFinder
                    isVisible={isFuzzyFinderVisible}
                    setIsVisible={setFuzzyFinderVisible}
                    isRepositoryRelatedPage={isRepositoryRelatedPage}
                    settingsCascade={props.settingsCascade}
                    telemetryService={props.telemetryService}
                    location={location}
                    userHistory={userHistory}
                />
            )}
            <SearchQueryStateObserver
                platformContext={props.platformContext}
                searchContextsEnabled={props.searchAggregationEnabled}
                setSelectedSearchContextSpec={props.setSelectedSearchContextSpec}
                selectedSearchContextSpec={props.selectedSearchContextSpec}
            />
        </div>
    )
}
