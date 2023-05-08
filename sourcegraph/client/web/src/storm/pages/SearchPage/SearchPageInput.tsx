import React, { FC, useCallback, useEffect, useRef } from 'react'

import { useLocation, useNavigate } from 'react-router-dom'
import { NavbarQueryState } from 'src/stores/navbarSearchQueryState'
import shallow from 'zustand/shallow'

import { SearchBox, Toggles } from '@sourcegraph/branded'
import { TraceSpanProvider } from '@sourcegraph/observability-client'
import {
    CaseSensitivityProps,
    SearchPatternTypeProps,
    SubmitSearchParameters,
    canSubmitSearch,
    QueryState,
    SearchModeProps,
    getUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/search'
import { useExperimentalFeatures } from '@sourcegraph/shared/src/settings/settings'
import { useIsLightTheme } from '@sourcegraph/shared/src/theme'
import { Form } from '@sourcegraph/wildcard'

import { Notices } from '../../../global/Notices'
import { useLegacyContext_onlyInStormRoutes } from '../../../LegacyRouteContext'
import { submitSearch } from '../../../search/helpers'
import { LazyExperimentalSearchInput } from '../../../search/input/LazyExperimentalSearchInput'
import { useRecentSearches } from '../../../search/input/useRecentSearches'
import { useExperimentalQueryInput } from '../../../search/useExperimentalSearchInput'
import { useNavbarQueryState, setSearchCaseSensitivity, setSearchPatternType, setSearchMode } from '../../../stores'

import styles from './SearchPageInput.module.scss'

// We want to prevent autofocus by default on devices with touch as their only input method.
// Touch only devices result in the onscreen keyboard not showing until the input loses focus and
// gets focused again by the user. The logic is not fool proof, but should rule out majority of cases
// where a touch enabled device has a physical keyboard by relying on detection of a fine pointer with hover ability.
const isTouchOnlyDevice =
    !window.matchMedia('(any-pointer:fine)').matches && window.matchMedia('(any-hover:none)').matches

const queryStateSelector = (
    state: NavbarQueryState
): Pick<CaseSensitivityProps, 'caseSensitive'> & SearchPatternTypeProps & Pick<SearchModeProps, 'searchMode'> => ({
    caseSensitive: state.searchCaseSensitivity,
    patternType: state.searchPatternType,
    searchMode: state.searchMode,
})

interface SearchPageInputProps {
    queryState: QueryState
    setQueryState: (newState: QueryState) => void
    hardCodedSearchContextSpec?: string
}

export const SearchPageInput: FC<SearchPageInputProps> = props => {
    const { queryState, setQueryState, hardCodedSearchContextSpec } = props

    const {
        authenticatedUser,
        isSourcegraphDotCom,
        telemetryService,
        platformContext,
        searchContextsEnabled,
        settingsCascade,
        selectedSearchContextSpec: dynamicSearchContextSpec,
        fetchSearchContexts,
        setSelectedSearchContextSpec,
    } = useLegacyContext_onlyInStormRoutes()

    const selectedSearchContextSpec = hardCodedSearchContextSpec || dynamicSearchContextSpec

    const location = useLocation()
    const navigate = useNavigate()

    const isLightTheme = useIsLightTheme()
    const { caseSensitive, patternType, searchMode } = useNavbarQueryState(queryStateSelector, shallow)
    const [experimentalQueryInput] = useExperimentalQueryInput()
    const applySuggestionsOnEnter =
        useExperimentalFeatures(features => features.applySearchQuerySuggestionOnEnter) ?? true

    const { recentSearches } = useRecentSearches()

    const submitSearchOnChange = useCallback(
        (parameters: Partial<SubmitSearchParameters> = {}) => {
            const query = parameters.query ?? queryState.query

            if (canSubmitSearch(query, selectedSearchContextSpec)) {
                submitSearch({
                    source: 'home',
                    query,
                    historyOrNavigate: navigate,
                    location,
                    patternType,
                    caseSensitive,
                    searchMode,
                    // In the new query input, context is either omitted (-> global)
                    // or explicitly specified.
                    selectedSearchContextSpec: experimentalQueryInput ? undefined : selectedSearchContextSpec,
                    ...parameters,
                })
            }
        },
        [
            queryState.query,
            selectedSearchContextSpec,
            navigate,
            location,
            patternType,
            caseSensitive,
            searchMode,
            experimentalQueryInput,
        ]
    )
    const submitSearchOnChangeRef = useRef(submitSearchOnChange)
    useEffect(() => {
        submitSearchOnChangeRef.current = submitSearchOnChange
    }, [submitSearchOnChange])

    const onSubmit = useCallback(
        (event?: React.FormEvent): void => {
            event?.preventDefault()
            submitSearchOnChangeRef.current()
        },
        [submitSearchOnChangeRef]
    )

    // TODO (#48103): Remove/simplify when new search input is released
    const input = experimentalQueryInput ? (
        <LazyExperimentalSearchInput
            telemetryService={telemetryService}
            patternType={patternType}
            interpretComments={false}
            queryState={queryState}
            onChange={setQueryState}
            onSubmit={onSubmit}
            isLightTheme={isLightTheme}
            platformContext={platformContext}
            authenticatedUser={authenticatedUser}
            fetchSearchContexts={fetchSearchContexts}
            getUserSearchContextNamespaces={getUserSearchContextNamespaces}
            isSourcegraphDotCom={isSourcegraphDotCom}
            submitSearch={submitSearchOnChange}
            selectedSearchContextSpec={selectedSearchContextSpec}
            className="flex-grow-1"
        >
            <Toggles
                patternType={patternType}
                caseSensitive={caseSensitive}
                setPatternType={setSearchPatternType}
                setCaseSensitivity={setSearchCaseSensitivity}
                searchMode={searchMode}
                setSearchMode={setSearchMode}
                settingsCascade={settingsCascade}
                navbarSearchQuery={queryState.query}
                showCopyQueryButton={false}
                showSmartSearchButton={false}
                structuralSearchDisabled={window.context?.experimentalFeatures?.structuralSearch === 'disabled'}
            />
        </LazyExperimentalSearchInput>
    ) : (
        <SearchBox
            platformContext={platformContext}
            getUserSearchContextNamespaces={getUserSearchContextNamespaces}
            fetchSearchContexts={fetchSearchContexts}
            selectedSearchContextSpec={selectedSearchContextSpec}
            setSelectedSearchContextSpec={setSelectedSearchContextSpec}
            telemetryService={telemetryService}
            authenticatedUser={authenticatedUser}
            isSourcegraphDotCom={isSourcegraphDotCom}
            settingsCascade={settingsCascade}
            searchContextsEnabled={searchContextsEnabled}
            showSearchContext={searchContextsEnabled}
            showSearchContextManagement={true}
            caseSensitive={caseSensitive}
            patternType={patternType}
            setPatternType={setSearchPatternType}
            setCaseSensitivity={setSearchCaseSensitivity}
            searchMode={searchMode}
            setSearchMode={setSearchMode}
            queryState={queryState}
            onChange={setQueryState}
            onSubmit={onSubmit}
            autoFocus={!isTouchOnlyDevice}
            isExternalServicesUserModeAll={window.context.externalServicesUserMode === 'all'}
            structuralSearchDisabled={window.context?.experimentalFeatures?.structuralSearch === 'disabled'}
            applySuggestionsOnEnter={applySuggestionsOnEnter}
            showSearchHistory={true}
            recentSearches={recentSearches}
        />
    )
    return (
        <div className="d-flex flex-row flex-shrink-past-contents">
            <Form className="flex-grow-1 flex-shrink-past-contents" onSubmit={onSubmit}>
                <div data-search-page-input-container={true} className={styles.inputContainer}>
                    <TraceSpanProvider name="SearchBox">
                        <div className="d-flex flex-grow-1 w-100">{input}</div>
                    </TraceSpanProvider>
                </div>
                <Notices className="my-3 text-center" location="home" />
            </Form>
        </div>
    )
}
