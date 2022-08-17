import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { createAggregateError, memoizeObservable } from '@sourcegraph/common'
import { gql } from '@sourcegraph/http-client'
import { FetchFileParameters } from '@sourcegraph/search-ui'

import { HighlightedFileResult, HighlightedFileVariables, HighlightResponseFormat } from '../graphql-operations'
import { PlatformContext } from '../platform/context'
import { makeRepoURI } from '../util/url'

/**
 * Fetches the specified highlighted file line ranges (`FetchFileParameters.ranges`) and returns
 * them as a list of ranges, each describing a list of lines in the form of HTML table '<tr>...</tr>'.
 */
export const fetchHighlightedFileLineRanges = memoizeObservable(
    (
        {
            platformContext,
            format = HighlightResponseFormat.HTML_HIGHLIGHT,
            ...context
        }: FetchFileParameters & {
            platformContext: Pick<PlatformContext, 'requestGraphQL'>
        },
        force?: boolean
    ): Observable<string[][]> =>
        platformContext
            .requestGraphQL<HighlightedFileResult, HighlightedFileVariables>({
                request: gql`
                    query HighlightedFile(
                        $repoName: String!
                        $commitID: String!
                        $filePath: String!
                        $disableTimeout: Boolean!
                        $ranges: [HighlightLineRange!]!
                        $format: HighlightResponseFormat!
                    ) {
                        repository(name: $repoName) {
                            commit(rev: $commitID) {
                                file(path: $filePath) {
                                    isDirectory
                                    richHTML
                                    highlight(disableTimeout: $disableTimeout, format: $format) {
                                        aborted
                                        lineRanges(ranges: $ranges)
                                    }
                                }
                            }
                        }
                    }
                `,
                variables: {
                    ...context,
                    format,
                    disableTimeout: Boolean(context.disableTimeout),
                },
                mightContainPrivateInfo: true,
            })
            .pipe(
                map(({ data, errors }) => {
                    if (!data?.repository?.commit?.file?.highlight) {
                        throw createAggregateError(errors)
                    }
                    const file = data.repository.commit.file
                    if (file.isDirectory) {
                        return []
                    }
                    return file.highlight.lineRanges
                })
            ),
    context =>
        makeRepoURI(context) +
        `?disableTimeout=${String(context.disableTimeout)}&ranges=${context.ranges
            .map(range => `${range.startLine}:${range.endLine}`)
            .join(',')}&format=${context.format}`
)
