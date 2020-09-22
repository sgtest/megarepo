import React, { useMemo, useEffect, useState } from 'react'
import classNames from 'classnames'
import { PanelContainer } from './PanelContainer'
import { Observable } from 'rxjs'
import { AuthenticatedUser } from '../../auth'
import { useObservable } from '../../../../shared/src/util/useObservable'
import { parseSearchURLQuery } from '..'
import { parseSearchQuery } from '../../../../shared/src/search/parser/parser'
import { EventLogResult } from '../backend'
import { Link } from '../../../../shared/src/components/Link'
import { FilterType } from '../../../../shared/src/search/interactive/util'
import { FILTERS } from '../../../../shared/src/search/parser/filters'
import { LoadingPanelView } from './LoadingPanelView'
import { ShowMoreButton } from './ShowMoreButton'

export const RepositoriesPanel: React.FunctionComponent<{
    authenticatedUser: AuthenticatedUser | null
    fetchRecentSearches: (userId: string, first: number) => Observable<EventLogResult | null>
    className?: string
}> = ({ authenticatedUser, fetchRecentSearches, className }) => {
    // Use a larger page size because not every search may have a `repo:` filter, and `repo:` filters could often
    // be duplicated. Therefore, we fetch more searches to populate this panel.
    const pageSize = 50
    const [itemsToLoad, setItemsToLoad] = useState(pageSize)

    const loadingDisplay = <LoadingPanelView text="Loading recently searched repositories" />

    const emptyDisplay = (
        <div className="panel-container__empty-container text-muted">
            <small className="mb-2">
                <p className="mb-1">Recently searched repositories will be displayed here.</p>
                <p className="mb-1">
                    Search in repositories with the <strong>repo:</strong> filter:
                </p>
                <p className="mb-1 text-monospace">
                    <span className="search-keyword">repo:</span>sourcegraph/sourcegraph
                </p>
                <p className="mb-1">Add the code host to scope to a single repository:</p>
                <p className="mb-1 text-monospace">
                    <span className="search-keyword">repo:</span>^git\.local/my/repo$
                </p>
            </small>
        </div>
    )

    const [repoFilterValues, setRepoFilterValues] = useState<string[] | null>(null)

    const searchEventLogs = useObservable(
        useMemo(() => fetchRecentSearches(authenticatedUser?.id || '', itemsToLoad), [
            fetchRecentSearches,
            authenticatedUser?.id,
            itemsToLoad,
        ])
    )

    useEffect(() => {
        if (searchEventLogs) {
            const recentlySearchedRepos = processRepositories(searchEventLogs)
            setRepoFilterValues(recentlySearchedRepos)
        }
    }, [searchEventLogs])

    function loadMoreItems(): void {
        setItemsToLoad(current => current + pageSize)
    }

    const contentDisplay = (
        <div className="mt-2">
            <div className="d-flex mb-1">
                <small>Search</small>
            </div>
            {repoFilterValues?.map((repoFilterValue, index) => (
                <dd key={`${repoFilterValue}-${index}`} className="text-monospace text-break">
                    <Link to={`/search?q=repo:${repoFilterValue}`}>
                        <span className="search-keyword">repo:</span>
                        <span className="repositories-panel__search-value">{repoFilterValue}</span>
                    </Link>
                </dd>
            ))}
            {searchEventLogs?.pageInfo.hasNextPage && (
                <ShowMoreButton className="test-repositories-panel-show-more" onClick={loadMoreItems} />
            )}
        </div>
    )

    return (
        <PanelContainer
            className={classNames(className, 'repositories-panel')}
            title="Repositories"
            state={repoFilterValues ? (repoFilterValues.length > 0 ? 'populated' : 'empty') : 'loading'}
            loadingContent={loadingDisplay}
            populatedContent={contentDisplay}
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
        const url = new URL(node.url)
        const queryFromURL = parseSearchURLQuery(url.search)
        const parsedQuery = parseSearchQuery(queryFromURL || '')
        if (parsedQuery.type === 'success') {
            for (const member of parsedQuery.token.members) {
                if (
                    member.token.type === 'filter' &&
                    (member.token.filterType.token.value === FilterType.repo ||
                        member.token.filterType.token.value === FILTERS[FilterType.repo].alias)
                ) {
                    if (
                        member.token.filterValue?.token.type === 'literal' &&
                        !recentlySearchedRepos.includes(member.token.filterValue.token.value)
                    ) {
                        recentlySearchedRepos.push(member.token.filterValue.token.value)
                    }
                    if (
                        member.token.filterValue?.token.type === 'quoted' &&
                        !recentlySearchedRepos.includes(member.token.filterValue.token.quotedValue)
                    ) {
                        recentlySearchedRepos.push(member.token.filterValue.token.quotedValue)
                    }
                }
            }
        }
    }
    return recentlySearchedRepos
}
