import React, { useMemo, useState } from 'react'

import { mdiChevronDown, mdiChevronLeft } from '@mdi/js'
import classNames from 'classnames'

import { EventLogResult, fetchRecentSearches } from '@sourcegraph/search'
import { SyntaxHighlightedSearchQuery } from '@sourcegraph/search-ui'
import { LATEST_VERSION } from '@sourcegraph/shared/src/search/stream'
import { Icon, H5, useObservable } from '@sourcegraph/wildcard'

import { SearchPatternType } from '../../../../graphql-operations'
import { HistorySidebarProps } from '../HistorySidebarView'

import styles from '../../search/SearchSidebarView.module.scss'

export const RecentSearchesSection: React.FunctionComponent<React.PropsWithChildren<HistorySidebarProps>> = ({
    platformContext,
    extensionCoreAPI,
    authenticatedUser,
}) => {
    const itemsToLoad = 15
    const [collapsed, setCollapsed] = useState(false)

    const recentSearchesResult = useObservable(
        useMemo(() => fetchRecentSearches(authenticatedUser.id, itemsToLoad, platformContext), [
            authenticatedUser.id,
            itemsToLoad,
            platformContext,
        ])
    )

    const recentSearches: RecentSearch[] | null = useMemo(
        () => processRecentSearches(recentSearchesResult ?? undefined),
        [recentSearchesResult]
    )

    if (!recentSearches) {
        return null
    }

    const onSavedSearchClick = (query: string): void => {
        platformContext.telemetryService.log('VSCERecentSearchClick')
        extensionCoreAPI
            .streamSearch(query, {
                // Debt: using defaults here. The saved search should override these, though.
                caseSensitive: false,
                patternType: SearchPatternType.standard,
                version: LATEST_VERSION,
                trace: undefined,
            })
            .catch(error => {
                // TODO surface to user
                console.error('Error submitting search from Sourcegraph sidebar', error)
            })
    }

    return (
        <div className={styles.sidebarSection}>
            <button
                type="button"
                className={classNames('btn btn-outline-secondary', styles.sidebarSectionCollapseButton)}
                onClick={() => setCollapsed(!collapsed)}
                aria-label={`${collapsed ? 'Expand' : 'Collapse'} recent searches`}
            >
                <H5 className="flex-grow-1">Recent Searches</H5>
                <Icon className="mr-1" svgPath={collapsed ? mdiChevronLeft : mdiChevronDown} aria-hidden={true} />
            </button>

            {!collapsed && (
                <div className={classNames('p-1', styles.sidebarSectionList)}>
                    {recentSearches
                        .filter((search, index) => index < itemsToLoad)
                        .map(search => (
                            <div key={search.timestamp + search.searchText}>
                                <small className={styles.sidebarSectionListItem}>
                                    <button
                                        type="button"
                                        className="btn btn-link p-0 text-left text-decoration-none"
                                        onClick={() => onSavedSearchClick(search.searchText)}
                                    >
                                        <SyntaxHighlightedSearchQuery query={search.searchText} />
                                    </button>
                                </small>
                            </div>
                        ))}
                </div>
            )}
        </div>
    )
}

interface RecentSearch {
    count: number
    searchText: string
    timestamp: string
    url: string
}

function processRecentSearches(eventLogResult?: EventLogResult): RecentSearch[] | null {
    if (!eventLogResult) {
        return null
    }

    const recentSearches: RecentSearch[] = []

    for (const node of eventLogResult.nodes) {
        if (node.argument && node.url) {
            const parsedArguments = JSON.parse(node.argument)
            const searchText: string | undefined = parsedArguments?.code_search?.query_data?.combined

            if (searchText) {
                if (recentSearches.length > 0 && recentSearches[recentSearches.length - 1].searchText === searchText) {
                    recentSearches[recentSearches.length - 1].count += 1
                } else {
                    const parsedUrl = new URL(node.url)
                    recentSearches.push({
                        count: 1,
                        url: parsedUrl.pathname + parsedUrl.search, // Strip domain from URL so clicking on it doesn't reload page
                        searchText,
                        timestamp: node.timestamp,
                    })
                }
            }
        }
    }

    return recentSearches
}
