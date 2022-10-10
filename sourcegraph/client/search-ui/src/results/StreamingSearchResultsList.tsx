import React, { useCallback } from 'react'

import classNames from 'classnames'
import { useLocation } from 'react-router'
import { Observable } from 'rxjs'

import { HoverMerged } from '@sourcegraph/client-api'
import { Hoverifier } from '@sourcegraph/codeintellify'
import { SearchContextProps } from '@sourcegraph/search'
import {
    CommitSearchResult,
    RepoSearchResult,
    FileContentSearchResult,
    FilePathSearchResult,
    SymbolSearchResult,
    FetchFileParameters,
} from '@sourcegraph/search-ui'
import { ActionItemAction } from '@sourcegraph/shared/src/actions/ActionItem'
import { FilePrefetcher, PrefetchableFile } from '@sourcegraph/shared/src/components/PrefetchableFile'
import { displayRepoName } from '@sourcegraph/shared/src/components/RepoLink'
import { VirtualList } from '@sourcegraph/shared/src/components/VirtualList'
import { Controller as ExtensionsController } from '@sourcegraph/shared/src/extensions/controller'
import { HoverContext } from '@sourcegraph/shared/src/hover/HoverOverlay.types'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import {
    AggregateStreamingSearchResults,
    SearchMatch,
    getMatchUrl,
    getRevision,
} from '@sourcegraph/shared/src/search/stream'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'

import { smartSearchClickedEvent } from '../util/events'

import { NoResultsPage } from './NoResultsPage'
import { StreamingSearchResultFooter } from './StreamingSearchResultsFooter'
import { useItemsToShow } from './use-items-to-show'

import resultContainerStyles from '../components/LegacyResultContainer.module.scss'
import styles from './StreamingSearchResultsList.module.scss'

export interface StreamingSearchResultsListProps
    extends ThemeProps,
        SettingsCascadeProps,
        TelemetryProps,
        Pick<SearchContextProps, 'searchContextsEnabled'>,
        PlatformContextProps<'requestGraphQL'> {
    isSourcegraphDotCom: boolean
    results?: AggregateStreamingSearchResults
    allExpanded: boolean
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
    showSearchContext: boolean
    /** Clicking on a match opens the link in a new tab. */
    openMatchesInNewTab?: boolean
    /** Available to web app through JS Context */
    assetsRoot?: string

    extensionsController?: Pick<ExtensionsController, 'extHostAPI'> | null
    hoverifier?: Hoverifier<HoverContext, HoverMerged, ActionItemAction>
    /**
     * Latest run query. Resets scroll visibility state when changed.
     * For example, `location.search` on web.
     */
    executedQuery: string
    /**
     * Classname to be applied to the container of a search result.
     */
    resultClassName?: string

    /**
     * For A/B testing on Sourcegraph.com. To be removed at latest by 12/2022.
     */
    smartSearchEnabled?: boolean

    prefetchFile?: FilePrefetcher

    prefetchFileEnabled?: boolean
}

export const StreamingSearchResultsList: React.FunctionComponent<
    React.PropsWithChildren<StreamingSearchResultsListProps>
> = ({
    results,
    allExpanded,
    fetchHighlightedFileLineRanges,
    settingsCascade,
    telemetryService,
    isLightTheme,
    isSourcegraphDotCom,
    searchContextsEnabled,
    showSearchContext,
    assetsRoot,
    platformContext,
    extensionsController,
    hoverifier,
    openMatchesInNewTab,
    executedQuery,
    resultClassName,
    smartSearchEnabled: smartSearchEnabled,
    prefetchFile,
    prefetchFileEnabled,
}) => {
    const resultsNumber = results?.results.length || 0
    const { itemsToShow, handleBottomHit } = useItemsToShow(executedQuery, resultsNumber)
    const location = useLocation()

    const logSearchResultClicked = useCallback(
        (index: number, type: string) => {
            telemetryService.log('SearchResultClicked')

            // This data ends up in Prometheus and is not part of the ping payload.
            telemetryService.log('search.ranking.result-clicked', { index, type })

            // Lucky search A/B test events on Sourcegraph.com. To be removed at latest by 12/2022.
            if (
                smartSearchEnabled &&
                !(
                    results?.alert?.kind === 'smart-search-additional-results' ||
                    results?.alert?.kind === 'smart-search-pure-results'
                )
            ) {
                telemetryService.log('SearchResultClickedAutoNone')
            }

            if (
                smartSearchEnabled &&
                (results?.alert?.kind === 'smart-search-additional-results' ||
                    results?.alert?.kind === 'smart-search-pure-results') &&
                results?.alert?.title &&
                results.alert.proposedQueries
            ) {
                const event = smartSearchClickedEvent(
                    results.alert.kind,
                    results.alert.title,
                    results.alert.proposedQueries.map(entry => entry.description || '')
                )

                telemetryService.log(event)
            }
        },
        [telemetryService, results, smartSearchEnabled]
    )

    const renderResult = useCallback(
        (result: SearchMatch, index: number): JSX.Element => {
            switch (result.type) {
                case 'content':
                case 'symbol':
                case 'path':
                    return (
                        <PrefetchableFile
                            isPrefetchEnabled={prefetchFileEnabled}
                            prefetch={prefetchFile}
                            filePath={result.path}
                            revision={getRevision(result.branches, result.commit)}
                            repoName={result.repository}
                            // PrefetchableFile adds an extra wrapper, so we lift the <li> up and match the ResultContainer styles.
                            // Better approach would be to use `as` to avoid wrapping, but that requires a larger refactor of the
                            // child components than is worth doing right now for this experimental feature
                            className={resultContainerStyles.resultContainer}
                            as="li"
                        >
                            {result.type === 'content' && (
                                <FileContentSearchResult
                                    index={index}
                                    location={location}
                                    telemetryService={telemetryService}
                                    result={result}
                                    onSelect={() => logSearchResultClicked(index, 'fileMatch')}
                                    defaultExpanded={false}
                                    showAllMatches={false}
                                    allExpanded={allExpanded}
                                    fetchHighlightedFileLineRanges={fetchHighlightedFileLineRanges}
                                    repoDisplayName={displayRepoName(result.repository)}
                                    settingsCascade={settingsCascade}
                                    extensionsController={extensionsController}
                                    hoverifier={hoverifier}
                                    openInNewTab={openMatchesInNewTab}
                                    containerClassName={resultClassName}
                                />
                            )}
                            {result.type === 'symbol' && (
                                <SymbolSearchResult
                                    index={index}
                                    telemetryService={telemetryService}
                                    result={result}
                                    onSelect={() => logSearchResultClicked(index, 'symbolMatch')}
                                    fetchHighlightedFileLineRanges={fetchHighlightedFileLineRanges}
                                    repoDisplayName={displayRepoName(result.repository)}
                                    settingsCascade={settingsCascade}
                                    openInNewTab={openMatchesInNewTab}
                                    containerClassName={resultClassName}
                                />
                            )}
                            {result.type === 'path' && (
                                <FilePathSearchResult
                                    index={index}
                                    result={result}
                                    onSelect={() => logSearchResultClicked(index, 'filePathMatch')}
                                    repoDisplayName={displayRepoName(result.repository)}
                                    containerClassName={resultClassName}
                                    telemetryService={telemetryService}
                                />
                            )}
                        </PrefetchableFile>
                    )
                case 'commit':
                    return (
                        <CommitSearchResult
                            index={index}
                            result={result}
                            platformContext={platformContext}
                            onSelect={() => logSearchResultClicked(index, 'commit')}
                            openInNewTab={openMatchesInNewTab}
                            containerClassName={resultClassName}
                            as="li"
                        />
                    )
                case 'repo':
                    return (
                        <RepoSearchResult
                            index={index}
                            result={result}
                            onSelect={() => logSearchResultClicked(index, 'repo')}
                            containerClassName={resultClassName}
                            as="li"
                        />
                    )
            }
        },
        [
            prefetchFileEnabled,
            prefetchFile,
            location,
            telemetryService,
            allExpanded,
            fetchHighlightedFileLineRanges,
            settingsCascade,
            extensionsController,
            hoverifier,
            openMatchesInNewTab,
            resultClassName,
            platformContext,
            logSearchResultClicked,
        ]
    )

    return (
        <>
            <VirtualList<SearchMatch>
                as="ol"
                aria-label="Search results"
                className={classNames('mt-2 mb-0', styles.list)}
                itemsToShow={itemsToShow}
                onShowMoreItems={handleBottomHit}
                items={results?.results || []}
                itemProps={undefined}
                itemKey={itemKey}
                renderItem={renderResult}
            />

            {itemsToShow >= resultsNumber && (
                <StreamingSearchResultFooter results={results}>
                    <>
                        {results?.state === 'complete' && resultsNumber === 0 && (
                            <NoResultsPage
                                searchContextsEnabled={searchContextsEnabled}
                                isSourcegraphDotCom={isSourcegraphDotCom}
                                isLightTheme={isLightTheme}
                                telemetryService={telemetryService}
                                showSearchContext={showSearchContext}
                                assetsRoot={assetsRoot}
                            />
                        )}
                    </>
                </StreamingSearchResultFooter>
            )}
        </>
    )
}

function itemKey(item: SearchMatch): string {
    if (item.type === 'content') {
        const lineStart = item.chunkMatches
            ? item.chunkMatches.length > 0
                ? item.chunkMatches[0].contentStart.line
                : 0
            : 0
        return `file:${getMatchUrl(item)}:${lineStart}`
    }
    if (item.type === 'symbol') {
        return `file:${getMatchUrl(item)}`
    }
    return getMatchUrl(item)
}
