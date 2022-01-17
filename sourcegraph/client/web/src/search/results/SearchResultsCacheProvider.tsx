import { Remote } from 'comlink'
import { isEqual } from 'lodash'
import React, { createContext, Dispatch, SetStateAction, useContext, useEffect, useMemo, useState } from 'react'
import { useHistory } from 'react-router'
import { of } from 'rxjs'
import { throttleTime } from 'rxjs/operators'

import { transformSearchQuery } from '@sourcegraph/shared/src/api/client/search'
import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { AggregateStreamingSearchResults, StreamSearchOptions } from '@sourcegraph/shared/src/search/stream'
import { TelemetryService } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { useObservable } from '@sourcegraph/wildcard'

import { SearchStreamingProps } from '..'

interface CachedResults {
    results: AggregateStreamingSearchResults | undefined
    query: string
    options: StreamSearchOptions
}

const SearchResultsCacheContext = createContext<[CachedResults | null, Dispatch<SetStateAction<CachedResults | null>>]>(
    [null, () => null]
)

/**
 * Returns the cached value if the options have not changed.
 * Otherwise, executes a new search and caches the value once
 * the search completes.
 *
 * @param streamSearch Search function.
 * @param options Options to pass on to `streamSeach`. MUST be wrapped in `useMemo` for this to work.
 * @returns Search results, either from cache or from running a new search (updated as new streaming results come in).
 */
export function useCachedSearchResults(
    streamSearch: SearchStreamingProps['streamSearch'],
    query: string,
    options: StreamSearchOptions,
    extensionHostAPI: Promise<Remote<FlatExtensionHostAPI>>,
    telemetryService: TelemetryService
): AggregateStreamingSearchResults | undefined {
    const [cachedResults, setCachedResults] = useContext(SearchResultsCacheContext)

    const history = useHistory()

    const transformedQuery = useMemo(() => transformSearchQuery({ query, extensionHostAPIPromise: extensionHostAPI }), [
        query,
        extensionHostAPI,
    ])

    const results = useObservable(
        useMemo(() => {
            // If query and options have not changed, return cached value
            if (query === cachedResults?.query && isEqual(options, cachedResults?.options)) {
                return of(cachedResults?.results)
            }

            return streamSearch(transformedQuery, options).pipe(
                throttleTime(500, undefined, { leading: true, trailing: true })
            )
        }, [
            query,
            cachedResults?.query,
            cachedResults?.options,
            cachedResults?.results,
            options,
            streamSearch,
            transformedQuery,
        ])
    )

    // Add a history listener that resets cached results if a new search is made
    // with the same query (e.g. to force refresh when the search button is clicked).
    useEffect(() => {
        const unlisten = history.listen((location, action) => {
            if (location.pathname === '/search' && action === 'PUSH') {
                setCachedResults(null)
            }
        })

        return unlisten
    }, [history, setCachedResults])

    useEffect(() => {
        if (results?.state === 'complete') {
            setCachedResults({ results, query, options })
        }
        // Only update cached results if the results change
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [results])

    useEffect(() => {
        // In case of back/forward navigation, log if the cache is being used.
        const cacheExists = query === cachedResults?.query && isEqual(options, cachedResults?.options)

        if (history.action === 'POP') {
            telemetryService.log('SearchResultsCacheRetrieved', { cacheHit: cacheExists }, { cacheHit: cacheExists })
        }
        // Only log when query or options have changed
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [query, options])

    return results
}

export const SearchResultsCacheProvider: React.FunctionComponent<{}> = ({ children }) => {
    const cachedResultsState = useState<CachedResults | null>(null)

    return (
        <SearchResultsCacheContext.Provider value={cachedResultsState}>{children}</SearchResultsCacheContext.Provider>
    )
}
