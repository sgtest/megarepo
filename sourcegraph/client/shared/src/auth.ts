import { gql } from '@sourcegraph/http-client'

import { CurrentAuthStateResult } from './graphql-operations'

export const currentAuthStateQuery = gql`
    query CurrentAuthState {
        currentUser {
            __typename
            id
            databaseID
            username
            avatarURL
            displayName
            siteAdmin
            tags
            url
            settingsURL
            organizations {
                nodes {
                    __typename
                    id
                    name
                    displayName
                    url
                    settingsURL
                }
            }
            session {
                canSignOut
            }
            viewerCanAdminister
            tags
            tosAccepted
            searchable
            emails {
                email
                verified
                isPrimary
            }
            latestSettings {
                id
                contents
            }
        }
    }
`
export type AuthenticatedUser = NonNullable<CurrentAuthStateResult['currentUser']>
