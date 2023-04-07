import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { memoizeObservable } from '@sourcegraph/common'
import { dataOrThrowErrors, gql } from '@sourcegraph/http-client'
import { useExperimentalFeatures } from '@sourcegraph/shared/src/settings/settings'
import { makeRepoURI } from '@sourcegraph/shared/src/util/url'

import { requestGraphQL } from '../../backend/graphql'
import { BlobFileFields, BlobResult, BlobVariables, HighlightResponseFormat } from '../../graphql-operations'

/**
 * Makes sure that default values are applied consistently for the cache key and the `fetchBlob` function.
 */
const applyDefaultValuesToFetchBlobOptions = ({
    disableTimeout = false,
    format = HighlightResponseFormat.HTML_HIGHLIGHT,
    startLine = null,
    endLine = null,
    visibleIndexID = null,
    scipSnapshot = false,
    ...options
}: FetchBlobOptions): Required<FetchBlobOptions> => ({
    ...options,
    disableTimeout,
    format,
    startLine,
    endLine,
    visibleIndexID,
    scipSnapshot,
})

function fetchBlobCacheKey(options: FetchBlobOptions): string {
    const { disableTimeout, format, scipSnapshot, visibleIndexID } = applyDefaultValuesToFetchBlobOptions(options)

    return `${makeRepoURI(
        options
    )}?disableTimeout=${disableTimeout}&=${format}&snap=${scipSnapshot}&visible=${visibleIndexID}`
}

interface FetchBlobOptions {
    repoName: string
    revision: string
    filePath: string
    disableTimeout?: boolean
    format?: HighlightResponseFormat
    startLine?: number | null
    endLine?: number | null
    scipSnapshot?: boolean
    visibleIndexID?: string | null
}

export const fetchBlob = memoizeObservable(
    (
        options: FetchBlobOptions
    ): Observable<(BlobFileFields & { snapshot?: { offset: number; data: string }[] | null }) | null> => {
        const {
            repoName,
            revision,
            filePath,
            disableTimeout,
            format,
            startLine,
            endLine,
            scipSnapshot,
            visibleIndexID,
        } = applyDefaultValuesToFetchBlobOptions(options)

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
                    $snapshot: Boolean!
                    $visibleIndexID: ID!
                ) {
                    repository(name: $repoName) {
                        commit(rev: $revision) {
                            file(path: $filePath) {
                                ...BlobFileFields
                            }
                            blob(path: $filePath) @include(if: $snapshot) {
                                lsif {
                                    snapshot(indexID: $visibleIndexID) {
                                        offset
                                        data
                                    }
                                }
                            }
                        }
                    }
                }

                fragment BlobFileFields on File2 {
                    __typename
                    content(startLine: $startLine, endLine: $endLine)
                    richHTML(startLine: $startLine, endLine: $endLine)
                    highlight(
                        disableTimeout: $disableTimeout
                        format: $format
                        startLine: $startLine
                        endLine: $endLine
                    ) {
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
            {
                repoName,
                revision,
                filePath,
                disableTimeout,
                format,
                html,
                startLine,
                endLine,
                snapshot: scipSnapshot,
                visibleIndexID: visibleIndexID ?? '',
            }
        ).pipe(
            map(dataOrThrowErrors),
            map(data => {
                if (!data.repository?.commit) {
                    throw new Error('Commit not found')
                }

                if (!data.repository.commit.file) {
                    throw new Error('File not found')
                }

                return {
                    ...data.repository.commit.file,
                    snapshot: data.repository.commit.blob?.lsif?.snapshot,
                }
            })
        )
    },
    fetchBlobCacheKey
)

/**
 * Returns the preferred blob prefetch format.
 *
 * Note: This format should match the format used when the blob is 'normally' fetched. E.g. in `BlobPage.tsx`.
 */
export const usePrefetchBlobFormat = (): HighlightResponseFormat => {
    const { enableCodeMirror, enableLazyHighlighting } = useExperimentalFeatures(features => ({
        enableCodeMirror: features.enableCodeMirrorFileView ?? true,
        enableLazyHighlighting: features.enableLazyBlobSyntaxHighlighting ?? true,
    }))

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
