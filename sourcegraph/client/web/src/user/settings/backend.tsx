import type { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { createAggregateError } from '@sourcegraph/common'
import { gql } from '@sourcegraph/http-client'
import type { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { EVENT_LOGGER } from '@sourcegraph/shared/src/telemetry/web/eventLogger'

import { requestGraphQL } from '../../backend/graphql'
import type {
    SetUserEmailVerifiedResult,
    SetUserEmailVerifiedVariables,
    UpdatePasswordResult,
    UpdatePasswordVariables,
} from '../../graphql-operations'

export const UPDATE_PASSWORD = gql`
    mutation UpdatePassword($oldPassword: String!, $newPassword: String!) {
        updatePassword(oldPassword: $oldPassword, newPassword: $newPassword) {
            alwaysNil
        }
    }
`

export const CREATE_PASSWORD = gql`
    mutation CreatePassword($newPassword: String!) {
        createPassword(newPassword: $newPassword) {
            alwaysNil
        }
    }
`

export const userExternalAccountFragment = gql`
    fragment UserExternalAccountFields on ExternalAccount {
        id
        serviceID
        serviceType
        clientID
        publicAccountData {
            displayName
            login
            url
        }
    }
`

export const USER_EXTERNAL_ACCOUNTS = gql`
    query UserExternalAccountsWithAccountData($username: String!) {
        user(username: $username) {
            externalAccounts {
                nodes {
                    id
                    serviceID
                    serviceType
                    clientID
                    publicAccountData {
                        displayName
                        login
                        url
                    }
                }
            }
        }
    }
`

export function updatePassword(args: UpdatePasswordVariables): Observable<void> {
    return requestGraphQL<UpdatePasswordResult, UpdatePasswordVariables>(
        gql`
            mutation UpdatePassword($oldPassword: String!, $newPassword: String!) {
                updatePassword(oldPassword: $oldPassword, newPassword: $newPassword) {
                    alwaysNil
                }
            }
        `,
        args
    ).pipe(
        map(({ data, errors }) => {
            if (!data?.updatePassword) {
                EVENT_LOGGER.log('UpdatePasswordFailed')
                throw createAggregateError(errors)
            }
            EVENT_LOGGER.log('PasswordUpdated')
        })
    )
}

/**
 * Set the verification state for a user email address.
 *
 * @param user the user's GraphQL ID
 * @param email the email address to edit
 * @param verified the new verification state for the user email
 */
export function setUserEmailVerified(user: Scalars['ID'], email: string, verified: boolean): Observable<void> {
    return requestGraphQL<SetUserEmailVerifiedResult, SetUserEmailVerifiedVariables>(
        gql`
            mutation SetUserEmailVerified($user: ID!, $email: String!, $verified: Boolean!) {
                setUserEmailVerified(user: $user, email: $email, verified: $verified) {
                    alwaysNil
                }
            }
        `,
        { user, email, verified }
    ).pipe(
        map(({ data, errors }) => {
            if (!data || (errors && errors.length > 0)) {
                throw createAggregateError(errors)
            }
        })
    )
}
