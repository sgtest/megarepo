import classNames from 'classnames'
import * as H from 'history'
import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { Observable } from 'rxjs'
import { debounceTime } from 'rxjs/operators'

import { FetchFileParameters } from '@sourcegraph/shared/src/components/CodeExcerpt'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { updateFilters } from '@sourcegraph/shared/src/search/query/transformer'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { asError } from '@sourcegraph/shared/src/util/errors'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'
import { useRedesignToggle } from '@sourcegraph/shared/src/util/useRedesignToggle'

import {
    CaseSensitivityProps,
    PatternTypeProps,
    SearchStreamingProps,
    resolveVersionContext,
    ParsedSearchQueryProps,
    MutableVersionContextProps,
} from '../..'
import { AuthenticatedUser } from '../../../auth'
import { CodeMonitoringProps } from '../../../code-monitoring'
import { PageTitle } from '../../../components/PageTitle'
import { SavedSearchModal } from '../../../savedSearches/SavedSearchModal'
import { QueryState, submitSearch } from '../../helpers'
import { queryTelemetryData } from '../../queryTelemetry'
import { SearchAlert } from '../SearchAlert'
import { LATEST_VERSION } from '../SearchResults'
import { SearchResultsInfoBar } from '../SearchResultsInfoBar'
import { SearchResultTypeTabs } from '../SearchResultTypeTabs'
import { VersionContextWarning } from '../VersionContextWarning'

import { StreamingProgress } from './progress/StreamingProgress'
import { SearchSidebar } from './sidebar/SearchSidebar'
import styles from './StreamingSearchResults.module.scss'
import { StreamingSearchResultsFilterBars } from './StreamingSearchResultsFilterBars'
import { StreamingSearchResultsList } from './StreamingSearchResultsList'

export interface StreamingSearchResultsProps
    extends SearchStreamingProps,
        Pick<ParsedSearchQueryProps, 'parsedSearchQuery'>,
        Pick<PatternTypeProps, 'patternType'>,
        Pick<MutableVersionContextProps, 'versionContext' | 'availableVersionContexts' | 'previousVersionContext'>,
        Pick<CaseSensitivityProps, 'caseSensitive'>,
        SettingsCascadeProps,
        ExtensionsControllerProps<'executeCommand' | 'extHostAPI'>,
        PlatformContextProps<'forceUpdateTooltip' | 'settings'>,
        TelemetryProps,
        ThemeProps,
        CodeMonitoringProps {
    authenticatedUser: AuthenticatedUser | null
    location: H.Location
    history: H.History
    navbarSearchQueryState: QueryState

    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

export const StreamingSearchResults: React.FunctionComponent<StreamingSearchResultsProps> = props => {
    const {
        parsedSearchQuery: query,
        patternType,
        caseSensitive,
        versionContext,
        streamSearch,
        location,
        history,
        availableVersionContexts,
        previousVersionContext,
        authenticatedUser,
        telemetryService,
    } = props

    // Log view event on first load
    useEffect(
        () => {
            telemetryService.logViewEvent('SearchResults')
        },
        // Only log view on initial load
        // eslint-disable-next-line react-hooks/exhaustive-deps
        []
    )

    // Log search query event when URL changes
    useEffect(() => {
        const query_data = queryTelemetryData(query, caseSensitive)
        telemetryService.log('SearchResultsQueried', {
            code_search: { query_data },
        })
        if (query_data.query?.field_type && query_data.query.field_type.value_diff > 0) {
            telemetryService.log('DiffSearchResultsQueried')
        }
    }, [caseSensitive, query, telemetryService])

    const trace = useMemo(() => new URLSearchParams(location.search).get('trace') ?? undefined, [location.search])
    const results = useObservable(
        useMemo(
            () =>
                streamSearch({
                    query,
                    version: LATEST_VERSION,
                    patternType: patternType ?? SearchPatternType.literal,
                    caseSensitive,
                    versionContext: resolveVersionContext(versionContext, availableVersionContexts),
                    trace,
                }).pipe(debounceTime(500)),
            [streamSearch, query, patternType, caseSensitive, versionContext, availableVersionContexts, trace]
        )
    )

    // Log events when search completes or fails
    useEffect(() => {
        if (results?.state === 'complete') {
            telemetryService.log('SearchResultsFetched', {
                code_search: {
                    // 🚨 PRIVACY: never provide any private data in { code_search: { results } }.
                    results: {
                        results_count: results.results.length,
                        any_cloning: results.progress.skipped.some(skipped => skipped.reason === 'repository-cloning'),
                    },
                },
            })
        } else if (results?.state === 'error') {
            telemetryService.log('SearchResultsFetchFailed', {
                code_search: { error_message: asError(results.error).message },
            })
            console.error(results.error)
        }
    }, [results, telemetryService])

    const [allExpanded, setAllExpanded] = useState(false)
    const onExpandAllResultsToggle = useCallback(() => {
        setAllExpanded(oldValue => !oldValue)
        telemetryService.log(allExpanded ? 'allResultsExpanded' : 'allResultsCollapsed')
    }, [allExpanded, telemetryService])

    const [showSavedSearchModal, setShowSavedSearchModal] = useState(false)
    const onSaveQueryClick = useCallback(() => setShowSavedSearchModal(true), [])
    const onSaveQueryModalClose = useCallback(() => {
        setShowSavedSearchModal(false)
        telemetryService.log('SavedQueriesToggleCreating', { queries: { creating: false } })
    }, [telemetryService])

    const [showVersionContextWarning, setShowVersionContextWarning] = useState(false)
    useEffect(
        () => {
            const searchParameters = new URLSearchParams(location.search)
            const versionFromURL = searchParameters.get('c')

            if (searchParameters.has('from-context-toggle')) {
                // The query param `from-context-toggle` indicates that the version context
                // changed from the version context toggle. In this case, we don't warn
                // users that the version context has changed.
                searchParameters.delete('from-context-toggle')
                history.replace({
                    search: searchParameters.toString(),
                    hash: history.location.hash,
                })
                setShowVersionContextWarning(false)
            } else {
                setShowVersionContextWarning(
                    (availableVersionContexts && versionFromURL !== previousVersionContext) || false
                )
            }
        },
        // Only show warning when URL changes
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [location.search]
    )
    const onDismissVersionContextWarning = useCallback(() => setShowVersionContextWarning(false), [
        setShowVersionContextWarning,
    ])

    // Reset expanded state when new search is started
    useEffect(() => {
        setAllExpanded(false)
    }, [location.search])

    const onSearchAgain = useCallback(
        (additionalFilters: string[]) => {
            telemetryService.log('SearchSkippedResultsAgainClicked')
            submitSearch({
                ...props,
                query: applyAdditionalFilters(query, additionalFilters),
                source: 'excludedResults',
            })
        },
        [query, telemetryService, props]
    )

    const [isRedesignEnabled] = useRedesignToggle()

    return (
        <div className={classNames('test-search-results search-results', styles.streamingSearchResults)}>
            <PageTitle key="page-title" title={query} />

            {isRedesignEnabled ? (
                <SearchSidebar {...props} query={props.navbarSearchQueryState.query} />
            ) : (
                <StreamingSearchResultsFilterBars {...props} results={results} />
            )}
            <div className="search-results-list">
                <div className="d-lg-flex mb-2 align-items-end flex-wrap">
                    {!isRedesignEnabled && (
                        <SearchResultTypeTabs
                            {...props}
                            query={props.navbarSearchQueryState.query}
                            className="search-results-list__tabs"
                        />
                    )}

                    <SearchResultsInfoBar
                        {...props}
                        query={query}
                        resultsFound={results ? results.results.length > 0 : false}
                        className={classNames('flex-grow-1', { 'border-bottom': !isRedesignEnabled })}
                        allExpanded={allExpanded}
                        onExpandAllResultsToggle={onExpandAllResultsToggle}
                        onSaveQueryClick={onSaveQueryClick}
                        stats={
                            <StreamingProgress
                                progress={results?.progress || { durationMs: 0, matchCount: 0, skipped: [] }}
                                state={results?.state || 'loading'}
                                history={props.history}
                                onSearchAgain={onSearchAgain}
                                showTrace={!!trace}
                            />
                        }
                    />
                </div>

                {showVersionContextWarning && (
                    <VersionContextWarning
                        versionContext={versionContext}
                        onDismissWarning={onDismissVersionContextWarning}
                    />
                )}

                {showSavedSearchModal && (
                    <SavedSearchModal
                        {...props}
                        query={query}
                        authenticatedUser={authenticatedUser}
                        onDidCancel={onSaveQueryModalClose}
                    />
                )}

                {results?.alert && (
                    <SearchAlert
                        alert={results.alert}
                        caseSensitive={caseSensitive}
                        patternType={patternType}
                        versionContext={versionContext}
                    />
                )}

                <StreamingSearchResultsList {...props} results={results} allExpanded={allExpanded} />
            </div>
        </div>
    )
}

const applyAdditionalFilters = (query: string, additionalFilters: string[]): string => {
    let newQuery = query
    for (const filter of additionalFilters) {
        const fieldValue = filter.split(':', 2)
        newQuery = updateFilters(newQuery, fieldValue[0], fieldValue[1])
    }
    return newQuery
}
