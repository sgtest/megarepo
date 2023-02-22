import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { memoizeObservable } from '@sourcegraph/common'
import { dataOrThrowErrors, gql } from '@sourcegraph/http-client'
import { makeRepoURI } from '@sourcegraph/shared/src/util/url'

import { requestGraphQL } from '../../backend/graphql'
import {
    BlobFileFields,
    BlobResult,
    BlobStencilFields,
    BlobVariables,
    HighlightResponseFormat,
    StencilResult,
    StencilVariables,
} from '../../graphql-operations'
import { useExperimentalFeatures } from '../../stores'

/**
 * Makes sure that default values are applied consistently for the cache key and the `fetchBlob` function.
 */
const applyDefaultValuesToFetchBlobOptions = ({
    disableTimeout = false,
    format = HighlightResponseFormat.HTML_HIGHLIGHT,
    startLine = null,
    endLine = null,
    ...options
}: FetchBlobOptions): Required<FetchBlobOptions> => ({
    ...options,
    disableTimeout,
    format,
    startLine,
    endLine,
})

function fetchBlobCacheKey(options: FetchBlobOptions): string {
    const { disableTimeout, format } = applyDefaultValuesToFetchBlobOptions(options)

    return `${makeRepoURI(options)}?disableTimeout=${disableTimeout}&=${format}`
}

interface FetchBlobOptions {
    repoName: string
    revision: string
    filePath: string
    disableTimeout?: boolean
    format?: HighlightResponseFormat
    startLine?: number | null
    endLine?: number | null
}

export const fetchBlob = memoizeObservable((options: FetchBlobOptions): Observable<BlobFileFields | null> => {
    const { repoName, revision, filePath, disableTimeout, format, startLine, endLine } =
        applyDefaultValuesToFetchBlobOptions(options)

    // We only want to include HTML data if explicitly requested. We always
    // include LSIF because this is used for languages that are configured
    // to be processed with tree sitter (and is used when explicitly
    // requested via JSON_SCIP).
    const html = [HighlightResponseFormat.HTML_PLAINTEXT, HighlightResponseFormat.HTML_HIGHLIGHT].includes(format)
    return requestGraphQL<BlobResult, BlobVariables>(
        gql`
            query Blob(
                $repoName: String!
                $revision: String!
                $filePath: String!
                $disableTimeout: Boolean!
                $format: HighlightResponseFormat!
                $html: Boolean!
                $startLine: Int
                $endLine: Int
            ) {
                repository(name: $repoName) {
                    commit(rev: $revision) {
                        file(path: $filePath) {
                            ...BlobFileFields
                        }
                    }
                }
            }

            fragment BlobFileFields on File2 {
                __typename
                content(startLine: $startLine, endLine: $endLine)
                richHTML(startLine: $startLine, endLine: $endLine)
                highlight(disableTimeout: $disableTimeout, format: $format, startLine: $startLine, endLine: $endLine) {
                    aborted
                    html @include(if: $html)
                    lsif
                }
                totalLines
                ... on GitBlob {
                    lfs {
                        byteSize
                    }
                    externalURLs {
                        url
                        serviceKind
                    }
                }
            }
        `,
        { repoName, revision, filePath, disableTimeout, format, html, startLine, endLine }
    ).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.repository?.commit) {
                throw new Error('Commit not found')
            }

            return data.repository.commit.file
        })
    )
}, fetchBlobCacheKey)

/**
 * Returns the preferred blob prefetch format.
 *
 * Note: This format should match the format used when the blob is 'normally' fetched. E.g. in `BlobPage.tsx`.
 */
export const usePrefetchBlobFormat = (): HighlightResponseFormat => {
    const enableCodeMirror = useExperimentalFeatures(features => features.enableCodeMirrorFileView ?? true)
    const enableLazyHighlighting = useExperimentalFeatures(
        features => features.enableLazyBlobSyntaxHighlighting ?? false
    )

    /**
     * Highlighted blobs (Fast)
     *
     * TODO: For large files, `PLAINTEXT` can still be faster, this is another potential UX improvement.
     * Outstanding issue before this can be enabled: https://github.com/sourcegraph/sourcegraph/issues/41413
     */
    if (enableCodeMirror) {
        return HighlightResponseFormat.JSON_SCIP
    }

    /**
     * Plaintext blobs (Fast)
     */
    if (enableLazyHighlighting) {
        return HighlightResponseFormat.HTML_PLAINTEXT
    }

    /**
     * Highlighted blobs (Slow)
     */
    return HighlightResponseFormat.HTML_HIGHLIGHT
}

interface FetchStencilOptions {
    repoName: string
    revision: string
    filePath: string
}

export const fetchStencil = memoizeObservable((options: FetchStencilOptions): Observable<BlobStencilFields[]> => {
    const { repoName, revision, filePath } = applyDefaultValuesToFetchBlobOptions(options)

    return requestGraphQL<StencilResult, StencilVariables>(
        gql`
            query Stencil($repoName: String!, $revision: String!, $filePath: String!) {
                repository(name: $repoName) {
                    commit(rev: $revision) {
                        blob(path: $filePath) {
                            lsif {
                                stencil {
                                    ...BlobStencilFields
                                }
                            }
                        }
                    }
                }
            }

            fragment BlobStencilFields on Range {
                start {
                    line
                    character
                }
                end {
                    line
                    character
                }
            }
        `,
        { repoName, revision, filePath }
    ).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.repository?.commit) {
                throw new Error('Commit not found')
            }

            return data.repository.commit.blob?.lsif?.stencil || []
        })
    )
}, makeRepoURI)
