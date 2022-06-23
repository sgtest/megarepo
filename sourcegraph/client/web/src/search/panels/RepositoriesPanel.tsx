import React, { useCallback, useEffect, useState, useMemo } from 'react'

import { gql } from '@apollo/client'
import { VisuallyHidden } from '@reach/visually-hidden'
import classNames from 'classnames'
import { of } from 'rxjs'

import { SyntaxHighlightedSearchQuery } from '@sourcegraph/search-ui'
import { scanSearchQuery } from '@sourcegraph/shared/src/search/query/scanner'
import { isRepoFilter } from '@sourcegraph/shared/src/search/query/validate'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Link, Text, useObservable } from '@sourcegraph/wildcard'

import { parseSearchURLQuery } from '..'
import { streamComputeQuery } from '../../../../shared/src/search/stream'
import { AuthenticatedUser } from '../../auth'
import { RecentlySearchedRepositoriesFragment } from '../../graphql-operations'
import { useExperimentalFeatures } from '../../stores'
import { EventLogResult } from '../backend'

import { EmptyPanelContainer } from './EmptyPanelContainer'
import { HomePanelsFetchMore, RECENTLY_SEARCHED_REPOSITORIES_TO_LOAD } from './HomePanels'
import { LoadingPanelView } from './LoadingPanelView'
import { PanelContainer } from './PanelContainer'
import { ShowMoreButton } from './ShowMoreButton'
import { useFocusOnLoadedMore } from './useFocusOnLoadedMore'

import styles from './RecentSearchesPanel.module.scss'

interface Props extends TelemetryProps {
    className?: string
    authenticatedUser: AuthenticatedUser | null
    recentlySearchedRepositories: RecentlySearchedRepositoriesFragment | null
    fetchMore: HomePanelsFetchMore
}

export const recentlySearchedRepositoriesFragment = gql`
    fragment RecentlySearchedRepositoriesFragment on User {
        recentlySearchedRepositoriesLogs: eventLogs(
            first: $firstRecentlySearchedRepositories
            eventName: "SearchResultsQueried"
        ) {
            nodes {
                argument
                timestamp
                url
            }
            pageInfo {
                hasNextPage
            }
            totalCount
        }
    }
`

type ComputeParseResult = [{ kind: string; value: string }]

export const RepositoriesPanel: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    className,
    telemetryService,
    recentlySearchedRepositories,
    fetchMore,
    authenticatedUser,
}) => {
    const [searchEventLogs, setSearchEventLogs] = useState<
        null | RecentlySearchedRepositoriesFragment['recentlySearchedRepositoriesLogs']
    >(recentlySearchedRepositories?.recentlySearchedRepositoriesLogs ?? null)
    useEffect(() => setSearchEventLogs(recentlySearchedRepositories?.recentlySearchedRepositoriesLogs ?? null), [
        recentlySearchedRepositories?.recentlySearchedRepositoriesLogs,
    ])

    const [itemsToLoad, setItemsToLoad] = useState(RECENTLY_SEARCHED_REPOSITORIES_TO_LOAD)
    const [isLoadingMore, setIsLoadingMore] = useState(false)

    const logRepoClicked = useCallback(() => telemetryService.log('RepositoriesPanelRepoFilterClicked'), [
        telemetryService,
    ])

    const loadingDisplay = <LoadingPanelView text="Loading recently searched repositories" />

    const emptyDisplay = (
        <EmptyPanelContainer className="text-muted">
            <small className="mb-2">
                <Text className="mb-1">Recently searched repositories will be displayed here.</Text>
                <Text className="mb-1">
                    Search in repositories with the <strong>repo:</strong> filter:
                </Text>
                <Text className="mb-1">
                    <SyntaxHighlightedSearchQuery query="repo:sourcegraph/sourcegraph" />
                </Text>
                <Text className="mb-1">Add the code host to scope to a single repository:</Text>
                <Text className="mb-1">
                    <SyntaxHighlightedSearchQuery query="repo:^git\.local/my/repo$" />
                </Text>
            </small>
        </EmptyPanelContainer>
    )

    const [repoFilterValues, setRepoFilterValues] = useState<string[] | null>(null)
    const getItemRef = useFocusOnLoadedMore(repoFilterValues?.length ?? 0)

    useEffect(() => {
        if (searchEventLogs) {
            const recentlySearchedRepos = processRepositories(searchEventLogs)
            setRepoFilterValues(recentlySearchedRepos)
        }
    }, [searchEventLogs])

    useEffect(() => {
        // Only log the first load (when items to load is equal to the page size)
        if (repoFilterValues && itemsToLoad === RECENTLY_SEARCHED_REPOSITORIES_TO_LOAD) {
            telemetryService.log(
                'RepositoriesPanelLoaded',
                { empty: repoFilterValues.length === 0 },
                { empty: repoFilterValues.length === 0 }
            )
        }
    }, [repoFilterValues, telemetryService, itemsToLoad])

    async function loadMoreItems(): Promise<void> {
        telemetryService.log('RepositoriesPanelShowMoreClicked')
        const newItemsToLoad = itemsToLoad + RECENTLY_SEARCHED_REPOSITORIES_TO_LOAD
        setItemsToLoad(newItemsToLoad)

        setIsLoadingMore(true)
        const { data } = await fetchMore({
            firstRecentlySearchedRepositories: newItemsToLoad,
        })

        setIsLoadingMore(false)
        if (data === undefined) {
            return
        }
        const node = data.node
        if (node === null || node.__typename !== 'User') {
            return
        }
        setSearchEventLogs(node.recentlySearchedRepositoriesLogs)
    }

    const contentDisplay = (
        <>
            <table className={classNames('mt-2', styles.resultsTable)}>
                <thead>
                    <tr className={styles.resultsTableRow}>
                        <th>
                            <small>Search</small>
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {repoFilterValues?.map((repoFilterValue, index) => (
                        <tr
                            // The repo is not guaranteed to be unique on its own, so we use index as well.
                            // eslint-disable-next-line react/no-array-index-key
                            key={`${repoFilterValue}-${index}`}
                            className={classNames('text-monospace text-break', styles.resultsTableRow)}
                        >
                            <td>
                                <small>
                                    <Link
                                        to={`/search?q=repo:${repoFilterValue}`}
                                        ref={getItemRef(index)}
                                        onClick={logRepoClicked}
                                    >
                                        <SyntaxHighlightedSearchQuery query={`repo:${repoFilterValue}`} />
                                    </Link>
                                </small>
                            </td>
                        </tr>
                    ))}
                </tbody>
            </table>
            {isLoadingMore && <VisuallyHidden aria-live="polite">Loading more repositories</VisuallyHidden>}
            {searchEventLogs?.pageInfo.hasNextPage && (
                <ShowMoreButton className="test-repositories-panel-show-more" onClick={loadMoreItems} />
            )}
        </>
    )

    // Get the user's repos from their commit history and extract all unique repos
    const checkHomePanelsFeatureFlag = useExperimentalFeatures(features => features.homePanelsComputeSuggestions)
    const gitRepository = useObservable(
        useMemo(
            () =>
                checkHomePanelsFeatureFlag && authenticatedUser
                    ? streamComputeQuery(
                          `content:output((.|\n)* -> $repo) author:${authenticatedUser.email} type:commit after:"1 year ago" count:all`
                      )
                    : of([]),
            [authenticatedUser, checkHomePanelsFeatureFlag]
        )
    )

    const gitSet = useMemo(() => {
        let gitRepositoryParsedString: ComputeParseResult[] = []
        if (gitRepository) {
            gitRepositoryParsedString = gitRepository.map(value => JSON.parse(value) as ComputeParseResult)
        }
        const gitReposList = gitRepositoryParsedString?.flat()

        const gitSet = new Set<string>()
        if (gitReposList) {
            for (const git of gitReposList) {
                if (git.value) {
                    gitSet.add(git.value)
                }
            }
        }

        return gitSet
    }, [gitRepository])

    const gitHistoryDisplay = (
        <table className={classNames('mt-2', styles.resultsTable)}>
            <tbody>
                {Array.from(gitSet).map(repo => (
                    <tr key={`${repo}`} className={classNames('text-monospace text-break', styles.resultsTableRow)}>
                        <td>
                            <small>
                                <Link to={`/search?q=repo:${repo}`} onClick={logRepoClicked}>
                                    <SyntaxHighlightedSearchQuery query={`repo:${repo}`} />
                                </Link>
                            </small>
                        </td>
                    </tr>
                ))}
            </tbody>
        </table>
    )

    // Wait for both the search event logs and the git history to be loaded
    const isLoading = !gitRepository || !repoFilterValues
    // If neither search event logs or git history have items, then display the empty display
    const isEmpty = repoFilterValues?.length === 0 && gitSet.size === 0

    return (
        <PanelContainer
            className={classNames(className, 'repositories-panel')}
            title="Repositories"
            state={isLoading ? 'loading' : isEmpty ? 'empty' : 'populated'}
            loadingContent={loadingDisplay}
            populatedContent={gitSet.size > 0 ? gitHistoryDisplay : contentDisplay}
            emptyContent={emptyDisplay}
        />
    )
}

function processRepositories(eventLogResult: EventLogResult): string[] | null {
    if (!eventLogResult) {
        return null
    }

    const recentlySearchedRepos: string[] = []

    for (const node of eventLogResult.nodes) {
        if (node.url) {
            const url = new URL(node.url)
            const queryFromURL = parseSearchURLQuery(url.search)
            const scannedQuery = scanSearchQuery(queryFromURL || '')
            if (scannedQuery.type === 'success') {
                for (const token of scannedQuery.term) {
                    if (isRepoFilter(token) && token.value && !recentlySearchedRepos.includes(token.value.value)) {
                        recentlySearchedRepos.push(token.value.value)
                    }
                }
            }
        }
    }
    return recentlySearchedRepos
}
