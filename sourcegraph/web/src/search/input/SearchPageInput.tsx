import * as H from 'history'
import React, { useState, useCallback, useEffect, useMemo } from 'react'
import { InteractiveModeInput } from './interactive/InteractiveModeInput'
import { Form } from 'reactstrap'
import { SearchModeToggle } from './interactive/SearchModeToggle'
import { VersionContextDropdown } from '../../nav/VersionContextDropdown'
import { LazyMonacoQueryInput } from './LazyMonacoQueryInput'
import { KeyboardShortcutsProps } from '../../keyboardShortcuts/keyboardShortcuts'
import { SearchButton } from './SearchButton'
import { Link } from '../../../../shared/src/components/Link'
import { SearchScopes } from './SearchScopes'
import { QuickLinks } from '../QuickLinks'
import { Notices } from '../../global/Notices'
import { SettingsCascadeProps, isSettingsValid } from '../../../../shared/src/settings/settings'
import { Settings } from '../../schema/settings.schema'
import { ThemeProps } from '../../../../shared/src/theme'
import { ThemePreferenceProps } from '../../theme'
import { ActivationProps } from '../../../../shared/src/components/activation/Activation'
import {
    PatternTypeProps,
    CaseSensitivityProps,
    InteractiveSearchProps,
    CopyQueryButtonProps,
    OnboardingTourProps,
    parseSearchURLQuery,
} from '..'
import { eventLogger } from '../../tracking/eventLogger'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { VersionContextProps } from '../../../../shared/src/search/util'
import { VersionContext } from '../../schema/site.schema'
import { submitSearch, SubmitSearchParams } from '../helpers'
import {
    generateStepTooltip,
    createStep1Tooltip,
    stepCallbacks,
    HAS_SEEN_TOUR_KEY,
    HAS_CANCELLED_TOUR_KEY,
    defaultTourOptions,
} from './SearchOnboardingTour'
import { useLocalStorage } from '../../util/useLocalStorage'
import Shepherd from 'shepherd.js'
import { AuthenticatedUser } from '../../auth'
import { TelemetryProps } from '../../../../shared/src/telemetry/telemetryService'

interface Props
    extends SettingsCascadeProps<Settings>,
        ThemeProps,
        ThemePreferenceProps,
        ActivationProps,
        PatternTypeProps,
        CaseSensitivityProps,
        KeyboardShortcutsProps,
        TelemetryProps,
        ExtensionsControllerProps<'executeCommand' | 'services'>,
        PlatformContextProps<'forceUpdateTooltip' | 'settings'>,
        InteractiveSearchProps,
        CopyQueryButtonProps,
        Pick<SubmitSearchParams, 'source'>,
        VersionContextProps,
        OnboardingTourProps {
    authenticatedUser: AuthenticatedUser | null
    location: H.Location
    history: H.History
    isSourcegraphDotCom: boolean
    setVersionContext: (versionContext: string | undefined) => void
    availableVersionContexts: VersionContext[] | undefined
    /** Whether globbing is enabled for filters. */
    globbing: boolean
    /** Whether to display the interactive mode input centered on the page, as on the search homepage. */
    interactiveModeHomepageMode?: boolean
    /** A query fragment to appear at the beginning of the input. */
    queryPrefix?: string
    autoFocus?: boolean

    // For NavLinks
    authRequired?: boolean
    showCampaigns: boolean
}

export const SearchPageInput: React.FunctionComponent<Props> = (props: Props) => {
    /** The query cursor position and value entered by the user in the query input */
    const [userQueryState, setUserQueryState] = useState({
        query: props.queryPrefix ? props.queryPrefix : '',
        cursorPosition: props.queryPrefix ? props.queryPrefix.length : 0,
    })

    useEffect(() => {
        setUserQueryState({ query: props.queryPrefix || '', cursorPosition: props.queryPrefix?.length || 0 })
    }, [props.queryPrefix])

    const quickLinks =
        (isSettingsValid<Settings>(props.settingsCascade) && props.settingsCascade.final.quicklinks) || []

    const [hasSeenTour, setHasSeenTour] = useLocalStorage(HAS_SEEN_TOUR_KEY, false)
    const [hasCancelledTour, setHasCancelledTour] = useLocalStorage(HAS_CANCELLED_TOUR_KEY, false)

    // tourWasActive denotes whether the tour was ever active while this component was rendered, in order
    // for us to know whether to show the structural search informational step on the results page.
    const [tourWasActive, setTourWasActive] = useState(false)

    const isHomepage = useMemo(
        () => props.location.pathname === '/search' && !parseSearchURLQuery(props.location.search),
        [props.location.pathname, props.location.search]
    )

    const showOnboardingTour = props.showOnboardingTour && isHomepage

    const tour = useMemo(() => new Shepherd.Tour(defaultTourOptions), [])

    useEffect(() => {
        if (showOnboardingTour) {
            tour.addSteps([
                {
                    id: 'start-tour',
                    text: createStep1Tooltip(
                        tour,
                        () => {
                            setUserQueryState({ query: 'lang:', cursorPosition: 'lang:'.length })
                            tour.show('filter-lang')
                        },
                        () => {
                            setUserQueryState({ query: 'repo:', cursorPosition: 'repo:'.length })
                            tour.show('filter-repository')
                        }
                    ),
                    attachTo: {
                        element: '.search-page__input-container',
                        on: 'bottom',
                    },
                },
                {
                    id: 'filter-lang',
                    text: generateStepTooltip(tour, 'Type to filter the language autocomplete', 2, 5),
                    when: {
                        show() {
                            eventLogger.log('ViewedOnboardingTourFilterLangStep')
                        },
                    },
                    attachTo: {
                        element: '.search-page__input-container',
                        on: 'top',
                    },
                },
                {
                    id: 'filter-repository',
                    text: generateStepTooltip(
                        tour,
                        "Type the name of a repository you've used recently to filter the autocomplete list",
                        2,
                        5
                    ),
                    when: {
                        show() {
                            eventLogger.log('ViewedOnboardingTourFilterRepoStep')
                        },
                    },
                    attachTo: {
                        element: '.search-page__input-container',
                        on: 'top',
                    },
                },
                // This step requires examples to be generated based on the language selected by the user,
                // so the text is generated by the callback called when the previous is completed.
                {
                    id: 'add-query-term',
                    when: {
                        show() {
                            eventLogger.log('ViewedOnboardingTourAddQueryTermStep')
                        },
                    },
                    attachTo: {
                        element: '.search-page__input-container',
                        on: 'bottom',
                    },
                },
                {
                    id: 'submit-search',
                    text: generateStepTooltip(
                        tour,
                        'Use <kbd>return</kbd> or the search button to run your search',
                        4,
                        5
                    ),
                    when: {
                        show() {
                            eventLogger.log('ViewedOnboardingTourSubmitSearchStep')
                        },
                    },
                    attachTo: {
                        element: '.search-button',
                        on: 'top',
                    },
                    advanceOn: { selector: '.search-button__btn', event: 'click' },
                },
            ])
        }
    }, [tour, showOnboardingTour])

    useEffect(() => {
        if (showOnboardingTour && !hasCancelledTour && !hasSeenTour) {
            setTourWasActive(true)
            tour.start()
            eventLogger.log('ViewOnboardingTour')
        }
        return
    }, [tour, showOnboardingTour, hasCancelledTour, hasSeenTour])

    useEffect(
        () => () => {
            // End tour on unmount.
            if (tour.isActive()) {
                tour.complete()
            }
        },
        [tour]
    )

    useMemo(() => {
        tour.on('complete', () => {
            setHasSeenTour(true)
        })
        tour.on('cancel', () => {
            setHasCancelledTour(true)
            // If the user closed the tour, we don't want to show
            // any further popups, so set this to false.
            setTourWasActive(false)
        })
    }, [tour, setHasSeenTour, setHasCancelledTour])

    const onSubmit = useCallback(
        (event?: React.FormEvent<HTMLFormElement>): void => {
            event?.preventDefault()
            submitSearch({
                ...props,
                query: userQueryState.query,
                source: 'home',
                searchParameters: tourWasActive ? [{ key: 'onboardingTour', value: 'true' }] : undefined,
            })
        },
        [props, userQueryState.query, tourWasActive]
    )

    return (
        <div className="d-flex flex-row flex-shrink-past-contents">
            {props.splitSearchModes && props.interactiveSearchMode ? (
                <InteractiveModeInput
                    {...props}
                    navbarSearchState={userQueryState}
                    onNavbarQueryChange={setUserQueryState}
                    toggleSearchMode={props.toggleSearchMode}
                    lowProfile={false}
                    homepageMode={props.interactiveModeHomepageMode}
                />
            ) : (
                <>
                    <Form className="flex-grow-1 flex-shrink-past-contents" onSubmit={onSubmit}>
                        <div className="search-page__input-container">
                            {props.splitSearchModes && (
                                <SearchModeToggle {...props} interactiveSearchMode={props.interactiveSearchMode} />
                            )}
                            <VersionContextDropdown
                                history={props.history}
                                caseSensitive={props.caseSensitive}
                                patternType={props.patternType}
                                navbarSearchQuery={userQueryState.query}
                                versionContext={props.versionContext}
                                setVersionContext={props.setVersionContext}
                                availableVersionContexts={props.availableVersionContexts}
                            />
                            <LazyMonacoQueryInput
                                {...props}
                                hasGlobalQueryBehavior={true}
                                queryState={userQueryState}
                                onChange={setUserQueryState}
                                onSubmit={onSubmit}
                                autoFocus={props.autoFocus !== false}
                                tour={showOnboardingTour ? tour : undefined}
                                tourAdvanceStepCallbacks={stepCallbacks}
                            />
                            <SearchButton />
                        </div>
                        <div className="search-page__input-sub-container">
                            {!props.splitSearchModes && (
                                <Link className="btn btn-link btn-sm pl-0" to="/search/query-builder">
                                    Query builder
                                </Link>
                            )}
                            <SearchScopes
                                history={props.history}
                                query={userQueryState.query}
                                authenticatedUser={props.authenticatedUser}
                                settingsCascade={props.settingsCascade}
                                patternType={props.patternType}
                                versionContext={props.versionContext}
                            />
                        </div>
                        <QuickLinks quickLinks={quickLinks} className="search-page__input-sub-container" />
                        <Notices
                            className="my-3"
                            location="home"
                            settingsCascade={props.settingsCascade}
                            history={props.history}
                        />
                    </Form>
                </>
            )}
        </div>
    )
}
