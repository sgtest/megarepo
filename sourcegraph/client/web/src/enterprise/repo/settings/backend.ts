import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { gql, dataOrThrowErrors } from '@sourcegraph/shared/src/graphql/graphql'

import { requestGraphQL } from '../../../backend/graphql'
import {
    RepoPermissionsInfoFields,
    RepoPermissionsInfoResult,
    RepoPermissionsInfoVariables,
} from '../../../graphql-operations'

export function repoPermissionsInfo(repo: Scalars['ID']): Observable<RepoPermissionsInfoFields | null> {
    return requestGraphQL<RepoPermissionsInfoResult, RepoPermissionsInfoVariables>(
        gql`
            query RepoPermissionsInfo($repo: ID!) {
                node(id: $repo) {
                    __typename
                    ... on Repository {
                        permissionsInfo {
                            ...RepoPermissionsInfoFields
                        }
                    }
                }
            }

            fragment RepoPermissionsInfoFields on PermissionsInfo {
                syncedAt
                updatedAt
            }
        `,
        { repo }
    ).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.node) {
                throw new Error('Repository not found')
            }
            if (data.node.__typename !== 'Repository') {
                throw new Error(`Node is a ${data.node.__typename}, not a Repository`)
            }
            return data.node.permissionsInfo
        })
    )
}
