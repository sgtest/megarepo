/* eslint-disable id-length */
import { Observable, fromEvent, Subscription, OperatorFunction, pipe, Subscriber, Notification } from 'rxjs'
import { defaultIfEmpty, map, materialize, scan } from 'rxjs/operators'

import * as GQL from '@sourcegraph/shared/src/graphql/schema'
import { asError, ErrorLike, isErrorLike } from '@sourcegraph/shared/src/util/errors'

import { SearchPatternType } from '../graphql-operations'

// This is an initial proof of concept implementation of search streaming.
// The protocol and implementation is still in the design phase. Feel free to
// change anything and everything here. We are iteratively improving this
// until it is no longer a proof of concept and instead works well.

export type SearchEvent =
    | { type: 'matches'; data: Match[] }
    | { type: 'progress'; data: Progress }
    | { type: 'filters'; data: Filter[] }
    | { type: 'alert'; data: Alert }
    | { type: 'error'; data: ErrorLike }
    | { type: 'done'; data: {} }

type Match = FileMatch | RepositoryMatch | CommitMatch | FileSymbolMatch

interface FileMatch {
    type: 'file'
    name: string
    repository: string
    branches?: string[]
    version?: string
    lineMatches: LineMatch[]
}

interface LineMatch {
    line: string
    lineNumber: number
    offsetAndLengths: number[][]
}

interface FileSymbolMatch {
    type: 'symbol'
    name: string
    repository: string
    branches?: string[]
    version?: string
    symbols: SymbolMatch[]
}

interface SymbolMatch {
    url: string
    name: string
    containerName: string
    kind: string
}

type MarkdownText = string

/**
 * Our batch based client requests generic fields from GraphQL to represent repo and commit/diff matches.
 * We currently are only using it for commit. To simplify the PoC we are keeping this interface for commits.
 *
 * @see GQL.IGenericSearchResultInterface
 */
interface CommitMatch {
    type: 'commit'
    label: MarkdownText
    url: string
    detail: MarkdownText

    content: MarkdownText
    ranges: number[][]
}

export interface RepositoryMatch {
    type: 'repo'
    repository: string
    branches?: string[]
}

/**
 * An aggregate type representing a progress update.
 * Should be replaced when a new ones come in.
 */
export interface Progress {
    /**
     * The number of repositories matching the repo: filter. Is set once they
     * are resolved.
     */
    repositoriesCount?: number

    // The number of non-overlapping matches. If skipped is non-empty, then
    // this is a lower bound.
    matchCount: number

    // Wall clock time in milliseconds for this search.
    durationMs: number

    /**
     * A description of shards or documents that were skipped. This has a
     * deterministic ordering. More important reasons will be listed first. If
     * a search is repeated, the final skipped list will be the same.
     * However, within a search stream when a new skipped reason is found, it
     * may appear anywhere in the list.
     */
    skipped: Skipped[]

    // The URL of the trace for this query, if it exists.
    trace?: string
}

export interface Skipped {
    /**
     * Why a document/shard/repository was skipped. We group counts by reason.
     *
     * - document-match-limit :: we found too many matches in a document, so we stopped searching it.
     * - shard-match-limit :: we found too many matches in a shard/repository, so we stopped searching it.
     * - repository-limit :: we did not search a repository because the set of repositories to search was too large.
     * - shard-timeout :: we ran out of time before searching a shard/repository.
     * - repository-cloning :: we could not search a repository because it is not cloned.
     * - repository-missing :: we could not search a repository because it is not cloned and we failed to find it on the remote code host.
     * - excluded-fork :: we did not search a repository because it is a fork.
     * - excluded-archive :: we did not search a repository because it is archived.
     * - display :: we hit the display limit, so we stopped sending results from the backend.
     */
    reason:
        | 'document-match-limit'
        | 'shard-match-limit'
        | 'repository-limit'
        | 'shard-timedout'
        | 'repository-cloning'
        | 'repository-missing'
        | 'excluded-fork'
        | 'excluded-archive'
        | 'display'
        | 'error'
    /**
     * A short message. eg 1,200 timed out.
     */
    title: string
    /**
     * A message to show the user. Usually includes information explaining the reason,
     * count as well as a sample of the missing items.
     */
    message: string
    severity: 'info' | 'warn' | 'error'
    /**
     * a suggested query expression to remedy the skip. eg "archived:yes" or "timeout:2m".
     */
    suggested?: {
        title: string
        queryExpression: string
    }
}

export interface Filter {
    value: string
    label: string
    count: number
    limitHit: boolean
    kind: string
}

interface Alert {
    title: string
    description?: string | null
    proposedQueries: ProposedQuery[] | null
}

interface ProposedQuery {
    description?: string | null
    query: string
}

const toGQLLineMatch = (line: LineMatch): GQL.ILineMatch => ({
    __typename: 'LineMatch',
    limitHit: false,
    lineNumber: line.lineNumber,
    offsetAndLengths: line.offsetAndLengths,
    preview: line.line,
})

function toGQLFileMatchBase(fileMatch: FileMatch | FileSymbolMatch): GQL.IFileMatch {
    let revision = ''
    if (fileMatch.branches) {
        const branch = fileMatch.branches[0]
        if (branch !== '') {
            revision = branch
        }
    } else if (fileMatch.version) {
        revision = fileMatch.version
    }

    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
    const file: GQL.IGitBlob = {
        path: fileMatch.name,
        // /github.com/gorilla/mux@v1.7.2/-/blob/mux_test.go
        // TODO return in response?
        url: `/${fileMatch.repository}${revision ? '@' + revision : ''}/-/blob/${fileMatch.name}`,
        commit: {
            oid: fileMatch.version || '',
        },
    } as GQL.IGitBlob
    const repository = toGQLRepositoryMatch({
        type: 'repo',
        repository: fileMatch.repository,
        branches: fileMatch.branches,
    })

    const revisionSpec = revision
        ? ({
              __typename: 'GitRef',
              displayName: revision,
              url: '/' + fileMatch.repository + '@' + revision,
          } as GQL.IGitRef)
        : null

    return {
        __typename: 'FileMatch',
        file,
        repository,
        revSpec: revisionSpec,
        resource: fileMatch.name,
        symbols: [],
        lineMatches: [],
        limitHit: false,
    }
}

const toGQLFileMatch = (fm: FileMatch): GQL.IFileMatch => ({
    ...toGQLFileMatchBase(fm),
    lineMatches: fm.lineMatches.map(toGQLLineMatch),
})

function toGQLSymbol(symbol: SymbolMatch): GQL.ISymbol {
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
    return {
        __typename: 'Symbol',
        ...symbol,
    } as GQL.ISymbol
}

const toGQLSymbolMatch = (fm: FileSymbolMatch): GQL.IFileMatch => ({
    ...toGQLFileMatchBase(fm),
    symbols: fm.symbols.map(toGQLSymbol),
})

// eslint-disable-next-line @typescript-eslint/consistent-type-assertions
const toMarkdown = (text: string | MarkdownText): GQL.IMarkdown => ({ __typename: 'Markdown', text } as GQL.IMarkdown)

export const toMarkdownCodeHtml = (text: string | MarkdownText): GQL.IMarkdown => ({
    __typename: 'Markdown',
    html: text.replace(/^```[_a-z]*\n/i, '').replace(/```$/i, ''), // Remove Markdown code indicators to render code as plain text
    text, // The full result with Markdown code indicators is still needed as SearchResultMatch.tsx uses this to determine syntax highlighting
})

export function toGQLRepositoryMatch(repo: RepositoryMatch): GQL.IRepository {
    const branch = repo?.branches?.[0]
    const revision = branch ? `@${branch}` : ''
    const label = repo.repository + revision
    const url = '/' + encodeURI(label)

    // We only need to return the subset defined in IGenericSearchResultInterface
    const gqlRepo: unknown = {
        __typename: 'Repository',
        label: toMarkdown(`[${label}](${url})`),
        url,
        detail: toMarkdown('Repository match'),
        matches: [],
        name: repo.repository,
    }

    return gqlRepo as GQL.IRepository
}

function toGQLCommitMatch(commit: CommitMatch): GQL.ICommitSearchResult {
    const match: GQL.ISearchResultMatch = {
        __typename: 'SearchResultMatch',
        url: commit.url,
        body: toMarkdownCodeHtml(commit.content),
        highlights: commit.ranges.map(([line, character, length]) => ({
            __typename: 'Highlight',
            line,
            character,
            length,
        })),
    }

    // We only need to return the subset defined in IGenericSearchResultInterface
    const gqlCommit: Partial<GQL.ICommitSearchResult> = {
        __typename: 'CommitSearchResult',
        label: toMarkdown(commit.label),
        url: commit.url,
        detail: toMarkdown(commit.detail),
        matches: [match],
    }

    return gqlCommit as GQL.ICommitSearchResult
}

export type StreamingResultsState = 'loading' | 'error' | 'complete'

interface BaseAggregateResults {
    state: StreamingResultsState
    results: GQL.SearchResult[]
    alert?: Alert
    filters: Filter[]
    progress: Progress
}

interface SuccessfulAggregateResults extends BaseAggregateResults {
    state: 'loading' | 'complete'
}

interface ErrorAggregateResults extends BaseAggregateResults {
    state: 'error'
    error: Error
}

export type AggregateStreamingSearchResults = SuccessfulAggregateResults | ErrorAggregateResults

const emptyAggregateResults: AggregateStreamingSearchResults = {
    state: 'loading',
    results: [],
    filters: [],
    progress: {
        durationMs: 0,
        matchCount: 0,
        skipped: [],
    },
}

function toGQLSearchResult(match: Match): GQL.SearchResult {
    switch (match.type) {
        case 'file':
            return toGQLFileMatch(match)
        case 'repo':
            return toGQLRepositoryMatch(match)
        case 'commit':
            return toGQLCommitMatch(match)
        case 'symbol':
            return toGQLSymbolMatch(match)
    }
}

/**
 * Converts a stream of SearchEvents into AggregateStreamingSearchResults
 */
const switchAggregateSearchResults: OperatorFunction<SearchEvent, AggregateStreamingSearchResults> = pipe(
    materialize(),
    scan(
        (
            results: AggregateStreamingSearchResults,
            newEvent: Notification<SearchEvent>
        ): AggregateStreamingSearchResults => {
            switch (newEvent.kind) {
                case 'N': {
                    switch (newEvent.value?.type) {
                        case 'matches':
                            return {
                                ...results,
                                // Matches are additive
                                results: results.results.concat(newEvent.value.data.map(toGQLSearchResult)),
                            }

                        case 'progress':
                            return {
                                ...results,
                                // Progress updates replace
                                progress: newEvent.value.data,
                            }

                        case 'filters':
                            return {
                                ...results,
                                // New filter results replace all previous ones
                                filters: newEvent.value.data,
                            }

                        case 'alert':
                            return {
                                ...results,
                                alert: newEvent.value.data,
                            }

                        default:
                            return results
                    }
                }
                case 'E': {
                    // Add the error as an extra skipped item
                    const error = asError(newEvent.error)
                    const errorSkipped: Skipped = {
                        title: 'Error loading results',
                        message: error.message,
                        reason: 'error',
                        severity: 'error',
                    }
                    return {
                        ...results,
                        error,
                        progress: {
                            ...results.progress,
                            skipped: [errorSkipped, ...results.progress.skipped],
                        },
                        state: 'error',
                    }
                }
                case 'C':
                    return {
                        ...results,
                        state: 'complete',
                    }
                default:
                    return results
            }
        },
        emptyAggregateResults
    ),
    defaultIfEmpty(emptyAggregateResults as AggregateStreamingSearchResults)
)

const observeMessages = <T extends SearchEvent>(
    type: T['type'],
    eventSource: EventSource,
    observer: Subscriber<SearchEvent>
): Subscription =>
    fromEvent(eventSource, type)
        .pipe(
            map((event: Event) => {
                if (!(event instanceof MessageEvent)) {
                    throw new TypeError(`internal error: expected MessageEvent in streaming search ${type}`)
                }
                try {
                    const parsedData = JSON.parse(event.data) as T['data']
                    return parsedData
                } catch {
                    throw new Error(`Could not parse ${type} message data in streaming search`)
                }
            }),
            map(data => ({ type, data } as T))
        )
        .subscribe(observer)

type MessageHandler<EventType extends SearchEvent['type'] = SearchEvent['type']> = (
    type: EventType,
    eventSource: EventSource,
    observer: Subscriber<SearchEvent>
) => Subscription

const messageHandlers: {
    [EventType in SearchEvent['type']]: MessageHandler<EventType>
} = {
    done: (type, eventSource, observer) =>
        fromEvent(eventSource, type).subscribe(() => {
            observer.complete()
            eventSource.close()
        }),
    error: (type, eventSource, observer) =>
        fromEvent(eventSource, type).subscribe(event => {
            let error: ErrorLike | null = null
            if (event instanceof MessageEvent) {
                try {
                    error = JSON.parse(event.data) as ErrorLike
                } catch {
                    error = null
                }
            }

            if (isErrorLike(error)) {
                observer.error(error)
            } else {
                // The EventSource API can return a DOM event that is not an Error object
                // (e.g. doesn't have the message property), so we need to construct our own here.
                // See https://developer.mozilla.org/en-US/docs/Web/API/EventSource/error_event
                observer.error(
                    new Error(
                        'The connection was closed before your search was completed. This may be due to a problem with a firewall, VPN or proxy, or a failure with the Sourcegraph server.'
                    )
                )
            }
            eventSource.close()
        }),
    matches: observeMessages,
    progress: observeMessages,
    filters: observeMessages,
    alert: observeMessages,
}

export interface StreamSearchOptions {
    query: string
    version: string
    patternType: SearchPatternType
    caseSensitive: boolean
    versionContext: string | undefined
    trace: string | undefined
}

/**
 * Initiates a streaming search. This is a type safe wrapper around Sourcegraph's streaming search API (using Server Sent Events).
 * The observable will emit each event returned from the backend.
 *
 * @param query the search query to send to Sourcegraph's backend.
 */
function search({
    query,
    version,
    patternType,
    caseSensitive,
    versionContext,
    trace,
}: StreamSearchOptions): Observable<SearchEvent> {
    return new Observable<SearchEvent>(observer => {
        const parameters = [
            ['q', `${query} ${caseSensitive ? 'case:yes' : ''}`],
            ['v', version],
            ['t', patternType as string],
            ['display', '500'],
        ]
        if (versionContext) {
            parameters.push(['vc', versionContext])
        }
        if (trace) {
            parameters.push(['trace', trace])
        }
        const parameterEncoded = parameters.map(([k, v]) => k + '=' + encodeURIComponent(v)).join('&')

        const eventSource = new EventSource('/search/stream?' + parameterEncoded)
        const subscriptions = new Subscription()
        for (const [eventType, handleMessages] of Object.entries(messageHandlers)) {
            subscriptions.add(
                (handleMessages as MessageHandler)(eventType as SearchEvent['type'], eventSource, observer)
            )
        }
        return () => {
            subscriptions.unsubscribe()
            eventSource.close()
        }
    })
}

/** Initiate a streaming search and aggregate the results */
export function aggregateStreamingSearch(options: StreamSearchOptions): Observable<AggregateStreamingSearchResults> {
    return search(options).pipe(switchAggregateSearchResults)
}
