import React from 'react'

import { useLocation } from 'react-router'

import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { LayoutRouteComponentProps } from '../routes'

import { parseSearchURLQuery } from '.'

const SearchPage = lazyComponent(() => import('./home/SearchPage'), 'SearchPage')
const StreamingSearchResults = lazyComponent(() => import('./results/StreamingSearchResults'), 'StreamingSearchResults')

/**
 * Renders the Search home page or Search results depending on whether a query
 * was submitted (present in the URL) or not.
 */
export const SearchPageWrapper: React.FunctionComponent<
    React.PropsWithChildren<LayoutRouteComponentProps<any>>
> = props => {
    const location = useLocation()
    const hasSearchQuery = parseSearchURLQuery(location.search)

    return hasSearchQuery ? <StreamingSearchResults {...props} /> : <SearchPage {...props} />
}
