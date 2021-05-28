import { subDays } from 'date-fns'
import { range } from 'lodash'
import { Observable, of } from 'rxjs'

import { ISearchContext } from '@sourcegraph/shared/src/graphql/schema'

import { ListSearchContextsResult, Maybe, Scalars } from '../graphql-operations'

export function mockFetchAutoDefinedSearchContexts(numberContexts = 0): Observable<ISearchContext[]> {
    return of(
        range(0, numberContexts).map(index => ({
            __typename: 'SearchContext',
            id: index.toString(),
            spec: `auto-defined-${index}`,
            name: `auto-defined-${index}`,
            namespace: null,
            public: true,
            autoDefined: true,
            viewerCanManage: false,
            description: 'Repositories on Sourcegraph',
            repositories: [],
            updatedAt: subDays(new Date(), 1).toISOString(),
        })) as ISearchContext[]
    )
}

export function mockFetchSearchContexts({
    first,
    query,
    after,
}: {
    first: number
    query?: string
    after?: string
}): Observable<ListSearchContextsResult['searchContexts']> {
    const result: ListSearchContextsResult['searchContexts'] = {
        nodes: [],
        pageInfo: {
            endCursor: null,
            hasNextPage: false,
        },
        totalCount: 0,
    }
    return of(result)
}

export function mockGetUserSearchContextNamespaces(): Maybe<Scalars['ID']>[] {
    return []
}
