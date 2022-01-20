import classNames from 'classnames'
import React, { ReactElement, useCallback } from 'react'

import {
    BuildSearchQueryURLParameters,
    QueryChangeSource,
    QueryState,
    SearchContextProps,
    createQueryExampleFromString,
    updateQueryWithFilterAndExample,
} from '@sourcegraph/search'
import { FilterType } from '@sourcegraph/shared/src/search/query/filters'
import { updateFilter } from '@sourcegraph/shared/src/search/query/transformer'
import { containsLiteralOrPattern } from '@sourcegraph/shared/src/search/query/validate'
import { SearchType } from '@sourcegraph/shared/src/search/stream'
import { Button, Link } from '@sourcegraph/wildcard'

import styles from './SearchSidebarSection.module.scss'

export interface SearchTypeLinksProps extends Pick<SearchContextProps, 'selectedSearchContextSpec'> {
    query: string
    onNavbarQueryChange: (queryState: QueryState) => void
    buildSearchURLQueryFromQueryState: (queryParameters: BuildSearchQueryURLParameters) => string
}

interface SearchTypeLinkProps extends SearchTypeLinksProps {
    type: SearchType
    children: string
}

/**
 * SearchTypeLink renders to a Link which immediately triggers a new search when
 * clicked.
 */
const SearchTypeLink: React.FunctionComponent<SearchTypeLinkProps> = ({
    type,
    query,
    selectedSearchContextSpec,
    children,
    buildSearchURLQueryFromQueryState,
}) => {
    const builtURLQuery = buildSearchURLQueryFromQueryState({
        query: updateFilter(query, FilterType.type, type as string),
        searchContextSpec: selectedSearchContextSpec,
    })

    return (
        <Link to={{ pathname: '/search', search: builtURLQuery }} className={styles.sidebarSectionListItem}>
            {children}
        </Link>
    )
}

interface SearchTypeButtonProps {
    children: string
    onClick: () => void
}

/**
 * SearchTypeButton renders to a button which updates the query state without
 * triggering a search. This allows users to adjust the query.
 */
const SearchTypeButton: React.FunctionComponent<SearchTypeButtonProps> = ({ children, onClick }) => (
    <Button
        className={classNames(styles.sidebarSectionListItem, styles.sidebarSectionButtonLink, 'flex-1')}
        value={children}
        onClick={onClick}
        variant="link"
    >
        {children}
    </Button>
)

/**
 * SearchSymbolButton either renders to a Link or a button, depending on whether
 * the search should be triggered immediately at click (if the query contains
 * patterns) or whether to allow the user to complete query and triggering it
 * themselves.
 */
const SearchSymbol: React.FunctionComponent<Omit<SearchTypeLinkProps, 'type'>> = props => {
    const type = 'symbol'
    const { query, onNavbarQueryChange } = props

    const setSymbolSearch = useCallback(() => {
        onNavbarQueryChange({
            query: updateFilter(query, FilterType.type, type),
        })
    }, [query, onNavbarQueryChange])

    if (containsLiteralOrPattern(query)) {
        return (
            <SearchTypeLink {...props} type={type}>
                {props.children}
            </SearchTypeLink>
        )
    }
    return <SearchTypeButton onClick={setSymbolSearch}>{props.children}</SearchTypeButton>
}

const repoExample = createQueryExampleFromString('{regexp-pattern}')

export const getSearchTypeLinks = (props: SearchTypeLinksProps): ReactElement[] => {
    function updateQueryWithRepoExample(): void {
        const updatedQuery = updateQueryWithFilterAndExample(props.query, FilterType.repo, repoExample, {
            singular: false,
            negate: false,
            emptyValue: true,
        })
        props.onNavbarQueryChange({
            changeSource: QueryChangeSource.searchTypes,
            query: updatedQuery.query,
            selectionRange: updatedQuery.placeholderRange,
            revealRange: updatedQuery.filterRange,
            showSuggestions: true,
        })
    }

    return [
        <SearchTypeButton onClick={updateQueryWithRepoExample} key="repo">
            Search repos by org or name
        </SearchTypeButton>,
        <SearchSymbol {...props} key="symbol">
            Find a symbol
        </SearchSymbol>,
        <SearchTypeLink {...props} type="diff" key="diff">
            Search diffs
        </SearchTypeLink>,
        <SearchTypeLink {...props} type="commit" key="commit">
            Search commit messages
        </SearchTypeLink>,
    ]
}
