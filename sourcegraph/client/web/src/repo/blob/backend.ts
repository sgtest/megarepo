import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { dataOrThrowErrors, gql } from '@sourcegraph/shared/src/graphql/graphql'
import { memoizeObservable } from '@sourcegraph/shared/src/util/memoizeObservable'
import { ParsedRepoURI, makeRepoURI } from '@sourcegraph/shared/src/util/url'

import { requestGraphQL } from '../../backend/graphql'
import { BlobFileFields, BlobResult, BlobVariables } from '../../graphql-operations'

function fetchBlobCacheKey(parsed: ParsedRepoURI & { isLightTheme: boolean; disableTimeout: boolean }): string {
    return makeRepoURI(parsed) + String(parsed.isLightTheme) + String(parsed.disableTimeout)
}

export const fetchBlob = memoizeObservable(
    (args: {
        repoName: string
        commitID: string
        filePath: string
        isLightTheme: boolean
        disableTimeout: boolean
    }): Observable<BlobFileFields | null> =>
        requestGraphQL<BlobResult, BlobVariables>(
            gql`
                query Blob(
                    $repoName: String!
                    $commitID: String!
                    $filePath: String!
                    $isLightTheme: Boolean!
                    $disableTimeout: Boolean!
                ) {
                    repository(name: $repoName) {
                        commit(rev: $commitID) {
                            file(path: $filePath) {
                                ...BlobFileFields
                            }
                        }
                    }
                }

                fragment BlobFileFields on File2 {
                    content
                    richHTML
                    highlight(disableTimeout: $disableTimeout, isLightTheme: $isLightTheme) {
                        aborted
                        html
                    }
                }
            `,
            args
        ).pipe(
            map(dataOrThrowErrors),
            map(data => {
                if (!data.repository?.commit) {
                    throw new Error('Commit not found')
                }
                return data.repository.commit.file
            })
        ),
    fetchBlobCacheKey
)
