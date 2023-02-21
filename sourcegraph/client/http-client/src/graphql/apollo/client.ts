import { ApolloClient, createHttpLink, from, HttpOptions, InMemoryCache, NormalizedCacheObject } from '@apollo/client'
import { LocalStorageWrapper, CachePersistor } from 'apollo3-cache-persist'
import { PersistenceMapperFunction } from 'apollo3-cache-persist/lib/types'
import { once } from 'lodash'

import { checkOk } from '../../http-status-error'
import { buildGraphQLUrl } from '../graphql'
import { ConcurrentRequestsLink } from '../links/concurrent-requests-link'

interface GetGraphqlClientOptions {
    headers?: Record<string, string>
    isAuthenticated: boolean
    cache: InMemoryCache
    baseUrl?: string
    credentials?: 'include' | 'omit' | 'same-origin'
    persistenceMapper?: PersistenceMapperFunction
}

export type GraphQLClient = ApolloClient<NormalizedCacheObject>

/**
 * 🚨 SECURITY: Use two unique keys for authenticated and anonymous users
 * to avoid keeping private information in localStorage after logout.
 */
const getApolloPersistCacheKey = (isAuthenticated: boolean): string =>
    `apollo-cache-persist-${isAuthenticated ? 'authenticated' : 'anonymous'}`

export const getGraphQLClient = once(async (options: GetGraphqlClientOptions): Promise<GraphQLClient> => {
    const { headers, baseUrl, isAuthenticated, credentials, persistenceMapper, cache } = options
    const uri = buildGraphQLUrl({ baseUrl })

    if (persistenceMapper) {
        const persistor = new CachePersistor({
            cache,
            persistenceMapper,
            // Use max 4 MB for persistent cache. Leave 1 MB for other means out of 5 MB available.
            // If exceeded, persistence will pause and app will start up cold on next launch.
            maxSize: 1024 * 1024 * 4,
            key: getApolloPersistCacheKey(isAuthenticated),
            storage: new LocalStorageWrapper(window.localStorage),
        })

        // 🚨 SECURITY: Drop persisted cache item in case `isAuthenticated` value changed.
        localStorage.removeItem(getApolloPersistCacheKey(!isAuthenticated))
        await persistor.restore()
    }

    const apolloClient = new ApolloClient({
        uri,
        cache,
        defaultOptions: {
            /**
             * The default `fetchPolicy` is `cache-first`, which returns a cached response
             * and doesn't trigger cache update. This is undesirable default behavior because
             * we want to keep our cache updated to avoid confusing the user with stale data.
             * `cache-and-network` allows us to return a cached result right away and then update
             * all consumers with the fresh data from the network request.
             */
            watchQuery: {
                fetchPolicy: 'cache-and-network',
            },
            /**
             * `client.query()` returns promise, so it can only resolve one response.
             * Meaning we cannot return the cached result first and then update it with
             * the response from the network as it's done in `client.watchQuery()`.
             * So we always need to make a network request to get data unless another
             * `fetchPolicy` is specified in the `client.query()` call.
             */
            query: {
                fetchPolicy: 'network-only',
            },
        },
        link: from([
            new ConcurrentRequestsLink(),
            createHttpLink({
                uri: ({ operationName }) => `${uri}?${operationName}`,
                headers,
                credentials,
                fetch: customFetch,
            }),
        ]),
    })

    return Promise.resolve(apolloClient)
})

const customFetch: HttpOptions['fetch'] = (uri, options) => fetch(uri, options).then(checkOk)
