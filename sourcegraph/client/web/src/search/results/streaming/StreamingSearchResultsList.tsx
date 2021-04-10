import * as H from 'history'
import FileIcon from 'mdi-react/FileIcon'
import SourceCommitIcon from 'mdi-react/SourceCommitIcon'
import SourceRepositoryIcon from 'mdi-react/SourceRepositoryIcon'
import SourceRepositoryMultipleIcon from 'mdi-react/SourceRepositoryMultipleIcon'
import React, { useCallback, useEffect, useState } from 'react'
import { Observable } from 'rxjs'

import { FetchFileParameters } from '@sourcegraph/shared/src/components/CodeExcerpt'
import { FileMatch } from '@sourcegraph/shared/src/components/FileMatch'
import { displayRepoName } from '@sourcegraph/shared/src/components/RepoFileLink'
import { VirtualList } from '@sourcegraph/shared/src/components/VirtualList'
import * as GQL from '@sourcegraph/shared/src/graphql/schema'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'

import { SearchResult } from '../../../components/SearchResult'
import { eventLogger } from '../../../tracking/eventLogger'
import { AggregateStreamingSearchResults } from '../../stream'

import { StreamingSearchResultFooter } from './StreamingSearchResultsFooter'

const initialItemsToShow = 15
const incrementalItemsToShow = 10

interface StreamingSearchResultsListProps extends ThemeProps, SettingsCascadeProps, TelemetryProps {
    results?: AggregateStreamingSearchResults

    location: H.Location
    history: H.History

    allExpanded: boolean

    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

export const StreamingSearchResultsList: React.FunctionComponent<StreamingSearchResultsListProps> = ({
    results,
    location,
    history,
    isLightTheme,
    allExpanded,
    fetchHighlightedFileLineRanges,
    settingsCascade,
    telemetryService,
}) => {
    const [itemsToShow, setItemsToShow] = useState(initialItemsToShow)
    const onBottomHit = useCallback(
        () => setItemsToShow(items => Math.min(results?.results.length || 0, items + incrementalItemsToShow)),
        [results?.results.length]
    )

    // Reset scroll visibility state when new search is started
    useEffect(() => {
        setItemsToShow(initialItemsToShow)
    }, [location.search])

    const itemKey = useCallback((item: GQL.GenericSearchResultInterface | GQL.IFileMatch): string => {
        if (item.__typename === 'FileMatch') {
            return `file:${item.file.url}`
        }
        return item.url
    }, [])

    const logSearchResultClicked = useCallback(() => telemetryService.log('SearchResultClicked'), [telemetryService])

    const renderResult = useCallback(
        (result: GQL.GenericSearchResultInterface | GQL.IFileMatch): JSX.Element => {
            switch (result.__typename) {
                case 'FileMatch':
                    return (
                        <FileMatch
                            location={location}
                            eventLogger={eventLogger}
                            icon={result.lineMatches && result.lineMatches.length > 0 ? SourceRepositoryIcon : FileIcon}
                            result={result}
                            onSelect={logSearchResultClicked}
                            expanded={false}
                            showAllMatches={false}
                            isLightTheme={isLightTheme}
                            allExpanded={allExpanded}
                            fetchHighlightedFileLineRanges={fetchHighlightedFileLineRanges}
                            repoDisplayName={displayRepoName(result.repository.name)}
                            settingsCascade={settingsCascade}
                        />
                    )
                case 'CommitSearchResult':
                    return (
                        <SearchResult
                            icon={SourceCommitIcon}
                            result={result}
                            isLightTheme={isLightTheme}
                            history={history}
                        />
                    )
                case 'Repository':
                    return (
                        <SearchResult
                            icon={SourceRepositoryMultipleIcon}
                            result={result}
                            isLightTheme={isLightTheme}
                            history={history}
                        />
                    )
            }
        },
        [
            isLightTheme,
            history,
            location,
            logSearchResultClicked,
            allExpanded,
            fetchHighlightedFileLineRanges,
            settingsCascade,
        ]
    )

    return (
        <>
            <VirtualList<GQL.SearchResult>
                className="mt-2"
                itemsToShow={itemsToShow}
                onShowMoreItems={onBottomHit}
                items={results?.results || []}
                itemProps={undefined}
                itemKey={itemKey}
                renderItem={renderResult}
            />

            {itemsToShow >= (results?.results.length || 0) && <StreamingSearchResultFooter results={results} />}
        </>
    )
}
