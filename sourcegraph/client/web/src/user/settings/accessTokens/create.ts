import type { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { createAggregateError } from '@sourcegraph/common'
import { gql } from '@sourcegraph/http-client'

import { requestGraphQL } from '../../../backend/graphql'
import type { CreateAccessTokenResult, CreateAccessTokenVariables, Scalars } from '../../../graphql-operations'
import { eventLogger } from '../../../tracking/eventLogger'

export function createAccessToken(
    user: Scalars['ID'],
    scopes: string[],
    note: string
): Observable<CreateAccessTokenResult['createAccessToken']> {
    return requestGraphQL<CreateAccessTokenResult, CreateAccessTokenVariables>(
        gql`
            mutation CreateAccessToken($user: ID!, $scopes: [String!]!, $note: String!) {
                createAccessToken(user: $user, scopes: $scopes, note: $note) {
                    id
                    token
                }
            }
        `,
        { user, scopes, note }
    ).pipe(
        map(({ data, errors }) => {
            if (!data?.createAccessToken || (errors && errors.length > 0)) {
                eventLogger.log('CreateAccessTokenFailed')
                throw createAggregateError(errors)
            }
            eventLogger.log('AccessTokenCreated')
            return data.createAccessToken
        })
    )
}
