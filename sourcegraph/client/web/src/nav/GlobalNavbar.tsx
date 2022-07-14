import React, { useEffect, useMemo, useRef, useState } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import BarChartIcon from 'mdi-react/BarChartIcon'
import BookOutlineIcon from 'mdi-react/BookOutlineIcon'
import MagnifyIcon from 'mdi-react/MagnifyIcon'
import PuzzleOutlineIcon from 'mdi-react/PuzzleOutlineIcon'
import { of } from 'rxjs'
import { startWith } from 'rxjs/operators'

import { ContributableMenu } from '@sourcegraph/client-api'
import { isErrorLike } from '@sourcegraph/common'
import { SearchContextInputProps, isSearchContextSpecAvailable } from '@sourcegraph/search'
import { ActivationProps } from '@sourcegraph/shared/src/components/activation/Activation'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import {
    KeyboardShortcutsProps,
    KEYBOARD_SHORTCUT_SHOW_COMMAND_PALETTE,
    KEYBOARD_SHORTCUT_SWITCH_THEME,
} from '@sourcegraph/shared/src/keyboardShortcuts/keyboardShortcuts'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { Settings } from '@sourcegraph/shared/src/schema/settings.schema'
import { getGlobalSearchContextFilter } from '@sourcegraph/shared/src/search/query/query'
import { omitFilter } from '@sourcegraph/shared/src/search/query/transformer'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { buildGetStartedURL } from '@sourcegraph/shared/src/util/url'
import {
    useObservable,
    Button,
    Link,
    FeedbackPrompt,
    ButtonLink,
    PopoverTrigger,
    useWindowSize,
} from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { BatchChangesProps } from '../batches'
import { BatchChangesNavItem } from '../batches/BatchChangesNavItem'
import { CodeMonitoringLogo } from '../code-monitoring/CodeMonitoringLogo'
import { ActivationDropdown } from '../components/ActivationDropdown'
import { BrandLogo } from '../components/branding/BrandLogo'
import { WebCommandListPopoverButton } from '../components/shared'
import { useFeatureFlag } from '../featureFlags/useFeatureFlag'
import { useHandleSubmitFeedback, useRoutesMatch } from '../hooks'
import { CodeInsightsProps } from '../insights/types'
import { isCodeInsightsEnabled } from '../insights/utils/is-code-insights-enabled'
import { LayoutRouteProps } from '../routes'
import { EnterprisePageRoutes, PageRoutes } from '../routes.constants'
import { SearchNavbarItem } from '../search/input/SearchNavbarItem'
import { useExperimentalFeatures, useNavbarQueryState } from '../stores'
import { ThemePreferenceProps } from '../theme'
import { userExternalServicesEnabledFromTags } from '../user/settings/cloud-ga'
import { showDotComMarketing } from '../util/features'

import { NavDropdown, NavDropdownItem } from './NavBar/NavDropdown'
import { StatusMessagesNavItem } from './StatusMessagesNavItem'
import { UserNavItem } from './UserNavItem'

import { NavGroup, NavItem, NavBar, NavLink, NavActions, NavAction } from '.'

import styles from './GlobalNavbar.module.scss'

interface Props
    extends SettingsCascadeProps<Settings>,
        PlatformContextProps,
        ExtensionsControllerProps,
        KeyboardShortcutsProps,
        TelemetryProps,
        ThemeProps,
        ThemePreferenceProps,
        ActivationProps,
        SearchContextInputProps,
        CodeInsightsProps,
        BatchChangesProps {
    history: H.History
    location: H.Location<{ query: string }>
    authenticatedUser: AuthenticatedUser | null
    authRequired: boolean
    isSourcegraphDotCom: boolean
    showSearchBox: boolean
    routes: readonly LayoutRouteProps<{}>[]

    // Whether globbing is enabled for filters.
    globbing: boolean

    /**
     * Which variation of the global navbar to render.
     *
     * 'low-profile' renders the the navbar with no border or background. Used on the search
     * homepage.
     *
     * 'low-profile-with-logo' renders the low-profile navbar but with the homepage logo. Used on community search context pages.
     */
    variant: 'default' | 'low-profile' | 'low-profile-with-logo'

    minimalNavLinks?: boolean
    isSearchAutoFocusRequired?: boolean
    isRepositoryRelatedPage?: boolean
    branding?: typeof window.context.branding
}

/**
 * Calculates NavLink variant based whether current content fits into container or not.
 *
 * @param containerReference a reference to navbar container
 */
function useCalculatedNavLinkVariant(
    containerReference: React.MutableRefObject<HTMLDivElement | null>
): 'compact' | undefined {
    const [navLinkVariant, setNavLinkVariant] = useState<'compact'>()
    const { width } = useWindowSize()
    const [savedWindowWidth, setSavedWindowWidth] = useState<number>()
    useEffect(() => {
        const container = containerReference.current
        if (!container) {
            return
        }
        if (container.offsetWidth < container.scrollWidth) {
            setNavLinkVariant('compact')
            setSavedWindowWidth(width)
        } else if (savedWindowWidth && width > savedWindowWidth) {
            setNavLinkVariant(undefined)
        }
    }, [containerReference, savedWindowWidth, width])

    return navLinkVariant
}

const AnalyticsNavItem: React.FunctionComponent = () => {
    const [isAdminAnalyticsDisabled] = useFeatureFlag('admin-analytics-disabled', false)

    if (isAdminAnalyticsDisabled) {
        return null
    }

    return (
        <NavAction className="d-none d-sm-flex">
            <Link to="/site-admin/analytics/search" className={classNames('font-weight-medium', styles.link)}>
                Analytics
            </Link>
        </NavAction>
    )
}

export const GlobalNavbar: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    authRequired,
    showSearchBox,
    variant,
    isLightTheme,
    branding,
    location,
    history,
    minimalNavLinks,
    isSourcegraphDotCom,
    isRepositoryRelatedPage,
    codeInsightsEnabled,
    searchContextsEnabled,
    ...props
}) => {
    // Workaround: can't put this in optional parameter value because of https://github.com/babel/babel/issues/11166
    branding = branding ?? window.context?.branding

    const query = useNavbarQueryState(state => state.searchQueryFromURL)

    const globalSearchContextSpec = useMemo(() => getGlobalSearchContextFilter(query), [query])

    // UI includes repositories section as part of the user navigation bar
    // This filter makes sure repositories feature flag is active.
    const showRepositorySection = props.authenticatedUser
        ? userExternalServicesEnabledFromTags(props.authenticatedUser.tags)
        : false

    const isSearchContextAvailable = useObservable(
        useMemo(
            () =>
                globalSearchContextSpec && searchContextsEnabled
                    ? // While we wait for the result of the `isSearchContextSpecAvailable` call, we assume the context is available
                      // to prevent flashing and moving content in the query bar. This optimizes for the most common use case where
                      // user selects a search context from the dropdown.
                      // See https://github.com/sourcegraph/sourcegraph/issues/19918 for more info.
                      isSearchContextSpecAvailable({
                          spec: globalSearchContextSpec.spec,
                          platformContext: props.platformContext,
                      }).pipe(startWith(true))
                    : of(false),
            [globalSearchContextSpec, searchContextsEnabled, props.platformContext]
        )
    )

    const routeMatch = useRoutesMatch(props.routes)
    const { handleSubmitFeedback } = useHandleSubmitFeedback({
        routeMatch,
    })

    const onNavbarQueryChange = useNavbarQueryState(state => state.setQueryState)
    const showSearchContext = useExperimentalFeatures(features => features.showSearchContext)
    const enableCodeMonitoring = useExperimentalFeatures(features => features.codeMonitoring)
    const showSearchNotebook = useExperimentalFeatures(features => features.showSearchNotebook)

    useEffect(() => {
        // On a non-search related page or non-repo page, we clear the query in
        // the main query input to avoid misleading users
        // that the query is relevant in any way on those pages.
        if (!showSearchBox) {
            onNavbarQueryChange({ query: '' })
            return
        }
        // Do nothing if there is no query in the URL
        if (!query) {
            return
        }

        // If a global search context spec is available to the user, we omit it from the
        // query and move it to the search contexts dropdown
        const finalQuery =
            globalSearchContextSpec && isSearchContextAvailable && showSearchContext
                ? omitFilter(query, globalSearchContextSpec.filter)
                : query

        onNavbarQueryChange({ query: finalQuery })
    }, [
        showSearchBox,
        onNavbarQueryChange,
        query,
        globalSearchContextSpec,
        isSearchContextAvailable,
        showSearchContext,
    ])

    const navbarReference = useRef<HTMLDivElement | null>(null)
    const navLinkVariant = useCalculatedNavLinkVariant(navbarReference)

    // CodeInsightsEnabled props controls insights appearance over OSS and Enterprise version
    // isCodeInsightsEnabled selector controls appearance based on user settings flags
    const codeInsights = codeInsightsEnabled && isCodeInsightsEnabled(props.settingsCascade)

    const searchNavBar = (
        <SearchNavbarItem
            {...props}
            location={location}
            history={history}
            isLightTheme={isLightTheme}
            isSourcegraphDotCom={isSourcegraphDotCom}
            searchContextsEnabled={searchContextsEnabled}
            isRepositoryRelatedPage={isRepositoryRelatedPage}
        />
    )

    const searchNavBarItems = useMemo(() => {
        const items: (NavDropdownItem | false)[] = [
            searchContextsEnabled &&
                !!showSearchContext && { path: EnterprisePageRoutes.Contexts, content: 'Contexts' },
        ]
        return items.filter<NavDropdownItem>((item): item is NavDropdownItem => !!item)
    }, [searchContextsEnabled, showSearchContext])

    return (
        <>
            <NavBar
                ref={navbarReference}
                logo={
                    <BrandLogo
                        branding={branding}
                        isLightTheme={isLightTheme}
                        variant="symbol"
                        className={styles.logo}
                    />
                }
            >
                <NavGroup>
                    <NavDropdown
                        toggleItem={{
                            path: PageRoutes.Search,
                            altPath: PageRoutes.RepoContainer,
                            icon: MagnifyIcon,
                            content: 'Code Search',
                            variant: navLinkVariant,
                        }}
                        routeMatch={routeMatch}
                        mobileHomeItem={{ content: 'Search home' }}
                        items={searchNavBarItems}
                    />
                    {showSearchNotebook && (
                        <NavItem icon={BookOutlineIcon}>
                            <NavLink variant={navLinkVariant} to={PageRoutes.Notebooks}>
                                Notebooks
                            </NavLink>
                        </NavItem>
                    )}
                    {enableCodeMonitoring && (
                        <NavItem icon={CodeMonitoringLogo}>
                            <NavLink variant={navLinkVariant} to="/code-monitoring">
                                Monitoring
                            </NavLink>
                        </NavItem>
                    )}
                    {/* This is the only circumstance where we show something
                         batch-changes-related even if the instance does not have batch
                         changes enabled, for marketing purposes on sourcegraph.com */}
                    {(props.batchChangesEnabled || isSourcegraphDotCom) && (
                        <BatchChangesNavItem variant={navLinkVariant} />
                    )}
                    {codeInsights && (
                        <NavItem icon={BarChartIcon}>
                            <NavLink variant={navLinkVariant} to="/insights">
                                Insights
                            </NavLink>
                        </NavItem>
                    )}
                    <NavItem icon={PuzzleOutlineIcon}>
                        <NavLink variant={navLinkVariant} to="/extensions">
                            Extensions
                        </NavLink>
                    </NavItem>
                    {props.activation && (
                        <NavItem>
                            <ActivationDropdown activation={props.activation} history={history} />
                        </NavItem>
                    )}
                </NavGroup>
                <NavActions>
                    {!props.authenticatedUser && (
                        <>
                            <NavAction>
                                <Link className={styles.link} to="https://about.sourcegraph.com">
                                    About <span className="d-none d-sm-inline">Sourcegraph</span>
                                </Link>
                            </NavAction>

                            {showDotComMarketing && (
                                <NavAction>
                                    <Link
                                        className={classNames('font-weight-medium', styles.link)}
                                        to="/help"
                                        target="_blank"
                                    >
                                        Docs
                                    </Link>
                                </NavAction>
                            )}
                        </>
                    )}
                    {props.authenticatedUser?.siteAdmin && <AnalyticsNavItem />}
                    {props.authenticatedUser && (
                        <NavAction>
                            <FeedbackPrompt onSubmit={handleSubmitFeedback} productResearchEnabled={true}>
                                <PopoverTrigger
                                    as={Button}
                                    aria-label="Feedback"
                                    variant="secondary"
                                    outline={true}
                                    size="sm"
                                    className={styles.feedbackTrigger}
                                >
                                    <span>Feedback</span>
                                </PopoverTrigger>
                            </FeedbackPrompt>
                        </NavAction>
                    )}
                    {props.authenticatedUser && (
                        <NavAction>
                            <WebCommandListPopoverButton
                                {...props}
                                location={location}
                                menu={ContributableMenu.CommandPalette}
                                keyboardShortcutForShow={KEYBOARD_SHORTCUT_SHOW_COMMAND_PALETTE}
                            />
                        </NavAction>
                    )}
                    {props.authenticatedUser &&
                        (props.authenticatedUser.siteAdmin ||
                            userExternalServicesEnabledFromTags(props.authenticatedUser.tags)) && (
                            <NavAction>
                                <StatusMessagesNavItem
                                    user={{
                                        id: props.authenticatedUser.id,
                                        username: props.authenticatedUser.username,
                                        isSiteAdmin: props.authenticatedUser?.siteAdmin || false,
                                    }}
                                    history={history}
                                />
                            </NavAction>
                        )}
                    {!props.authenticatedUser ? (
                        <>
                            <NavAction>
                                <div>
                                    <Button
                                        className="mr-1"
                                        to="/sign-in"
                                        variant="secondary"
                                        outline={true}
                                        size="sm"
                                        as={Link}
                                    >
                                        Log in
                                    </Button>
                                    <ButtonLink className={styles.signUp} to={buildGetStartedURL('nav')} size="sm">
                                        Get started
                                    </ButtonLink>
                                </div>
                            </NavAction>
                        </>
                    ) : (
                        <NavAction>
                            <UserNavItem
                                {...props}
                                isLightTheme={isLightTheme}
                                authenticatedUser={props.authenticatedUser}
                                showDotComMarketing={showDotComMarketing}
                                showRepositorySection={showRepositorySection}
                                codeHostIntegrationMessaging={
                                    (!isErrorLike(props.settingsCascade.final) &&
                                        props.settingsCascade.final?.['alerts.codeHostIntegrationMessaging']) ||
                                    'browser-extension'
                                }
                                keyboardShortcutForSwitchTheme={KEYBOARD_SHORTCUT_SWITCH_THEME}
                            />
                        </NavAction>
                    )}
                </NavActions>
            </NavBar>
            {showSearchBox && (
                <div className="w-100 px-3 pt-2">
                    <div className="pb-2 border-bottom">{searchNavBar}</div>
                </div>
            )}
        </>
    )
}
