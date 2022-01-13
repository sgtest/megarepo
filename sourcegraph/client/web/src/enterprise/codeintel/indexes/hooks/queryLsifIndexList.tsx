import { ApolloClient } from '@apollo/client'
import { from, Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { gql, getDocumentNode } from '@sourcegraph/http-client'
import * as GQL from '@sourcegraph/shared/src/schema'

import { LsifIndexesResult, LsifIndexesVariables, LsifIndexFields } from '../../../../graphql-operations'

import { lsifIndexFieldsFragment } from './types'

interface IndexConnection {
    nodes: LsifIndexFields[]
    totalCount: number | null
    pageInfo: { endCursor: string | null; hasNextPage: boolean }
}

const LSIF_INDEXES = gql`
    query LsifIndexes($state: LSIFIndexState, $first: Int, $after: String, $query: String) {
        lsifIndexes(query: $query, state: $state, first: $first, after: $after) {
            nodes {
                ...LsifIndexFields
            }
            totalCount
            pageInfo {
                endCursor
                hasNextPage
            }
        }
    }

    ${lsifIndexFieldsFragment}
`

export const queryLsifIndexList = (
    { query, state, first, after }: GQL.ILsifIndexesOnRepositoryArguments,
    client: ApolloClient<object>
): Observable<IndexConnection> => {
    const vars = {
        query: query ?? null,
        state: state ?? null,
        first: first ?? null,
        after: after ?? null,
    }

    return from(
        client.query<LsifIndexesResult, LsifIndexesVariables>({
            query: getDocumentNode(LSIF_INDEXES),
            variables: { ...vars },
        })
    ).pipe(
        map(({ data }) => data),
        map(({ lsifIndexes }) => lsifIndexes)
    )
}
