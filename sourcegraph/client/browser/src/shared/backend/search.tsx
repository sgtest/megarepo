import { flatten } from 'lodash'
import { Observable, Subject } from 'rxjs'
import {
    catchError,
    debounceTime,
    distinctUntilChanged,
    filter,
    map,
    mergeMap,
    publishReplay,
    refCount,
    repeat,
    switchMap,
    take,
    toArray,
} from 'rxjs/operators'
import * as GQL from '../../../../../shared/src/graphql/schema'
import { getContext } from './context'
import { createAggregateError } from './errors'
import { queryGraphQL } from './graphql'

interface BaseSuggestion {
    title: string
    description?: string

    /**
     * The URL that is navigated to when the user selects this
     * suggestion.
     */
    url: string

    /**
     * A label describing the action taken when navigating to
     * the URL (e.g., "go to repository").
     */
    urlLabel: string
}

interface SymbolSuggestion extends BaseSuggestion {
    type: 'symbol'
    kind: string
}

interface RepoSuggestion extends BaseSuggestion {
    type: 'repo'
}

interface FileSuggestion extends BaseSuggestion {
    type: 'file'
}

interface DirSuggestion extends BaseSuggestion {
    type: 'dir'
}

export type Suggestion = SymbolSuggestion | RepoSuggestion | FileSuggestion | DirSuggestion

/**
 * Returns all but the last element of path, or "." if that would be the empty path.
 */
function dirname(path: string): string | undefined {
    return (
        path
            .split('/')
            .slice(0, -1)
            .join('/') || '.'
    )
}

/**
 * Returns the last element of path, or "." if path is empty.
 */
function basename(path: string): string {
    return path.split('/').slice(-1)[0] || '.'
}

export function createSuggestion(item: GQL.SearchSuggestion): Suggestion | null {
    switch (item.__typename) {
        case 'Repository': {
            return {
                type: 'repo',
                title: item.name,
                url: item.url,
                urlLabel: 'go to repository',
            }
        }
        case 'File': {
            const descriptionParts: string[] = []
            const dir = dirname(item.path)
            if (dir !== undefined && dir !== '.') {
                descriptionParts.push(`${dir}/`)
            }
            descriptionParts.push(basename(item.repository.name))
            if (item.isDirectory) {
                return {
                    type: 'dir',
                    title: item.name,
                    description: descriptionParts.join(' — '),
                    url: item.url,
                    urlLabel: 'go to dir',
                }
            }
            return {
                type: 'file',
                title: item.name,
                description: descriptionParts.join(' — '),
                url: item.url,
                urlLabel: 'go to file',
            }
        }
        case 'Symbol': {
            return {
                type: 'symbol',
                kind: item.kind,
                title: item.name,
                description: `${item.containerName || item.location.resource.path} — ${basename(
                    item.location.resource.repository.name
                )}`,
                url: item.url,
                urlLabel: 'go to definition',
            }
        }
        default:
            return null
    }
}

const symbolsFragment = `
    fragment SymbolFields on Symbol {
        __typename
        name
        containerName
        url
        kind
        location {
            resource {
                path
                repository {
                    name
                }
            }
            url
        }
    }
`

export interface SearchOptions {
    /** The query entered by the user */
    query: string
}

const fetchSuggestions = (options: SearchOptions, first: number) =>
    queryGraphQL({
        ctx: getContext({ repoKey: '', isRepoSpecific: false }),
        request: `
            query SearchSuggestions($query: String!, $first: Int!) {
                search(query: $query) {
                    suggestions(first: $first) {
                        ... on Repository {
                            __typename
                            name
                            url
                        }
                        ... on File {
                            __typename
                            path
                            name
                            isDirectory
                            url
                            repository {
                                name
                            }
                        }
                        ... on Symbol {
                            ...SymbolFields
                        }
                    }
                }
            }
            ${symbolsFragment}
        `,
        variables: {
            query: options.query,
            // The browser extension API only takes 5 suggestions
            first,
        },
        retry: false,
    }).pipe(
        mergeMap(({ data, errors }) => {
            if (!data || !data.search || !data.search.suggestions) {
                throw createAggregateError(errors)
            }
            return data.search.suggestions
        })
    )

interface SuggestionInput {
    query: string
    handler: (suggestion: Suggestion[]) => void
}

export const createSuggestionFetcher = (first = 5) => {
    const fetcher = new Subject<SuggestionInput>()

    fetcher
        .pipe(
            distinctUntilChanged(),
            debounceTime(200),
            switchMap(({ query, handler }) => {
                const options: SearchOptions = {
                    query,
                }
                return fetchSuggestions(options, first).pipe(
                    take(first),
                    map(createSuggestion),
                    // createSuggestion will return null if we get a type we don't recognize
                    filter((f): f is Suggestion => !!f),
                    toArray(),
                    map((suggestions: Suggestion[]) => ({
                        suggestions,
                        suggestHandler: handler,
                    })),
                    publishReplay(),
                    refCount()
                )
            }),
            // But resubscribe afterwards
            repeat()
        )
        .subscribe(({ suggestions, suggestHandler }) => suggestHandler(suggestions))

    return (input: SuggestionInput) => fetcher.next(input)
}

export const fetchSymbols = (options: SearchOptions): Observable<GQL.ISymbol[]> =>
    queryGraphQL({
        ctx: getContext({ isRepoSpecific: true }),
        request: `
            query SearchResults($query: String!) {
                search(query: $query) {
                    results {
                        results {
                            ... on FileMatch {
                                symbols {
                                    ...SymbolFields
                                }
                            }
                        }
                    }
                }
            }
            ${symbolsFragment}
        `,
        variables: {
            query: options.query,
        },
        retry: false,
    }).pipe(
        map(({ data, errors }) => {
            if (!data || !data.search || !data.search.results || !data.search.results.results) {
                throw createAggregateError(errors)
            }

            const symbolsResults = flatten(
                (data.search.results.results as GQL.IFileMatch[]).map(match => match.symbols)
            )

            return symbolsResults
        }),
        catchError(err => {
            // TODO@ggilmore: This is a kludge that should be removed once the
            // code smells with requestGraphQL are addressed.
            // At this time of writing, requestGraphQL throws the entire response
            // instead of a well-formed error created from response.errors. This kludge
            // manually creates this well-formed error before re-throwing it.
            //
            // See https://github.com/sourcegraph/browser-extension/pull/235 for more context.

            if (err.errors) {
                throw createAggregateError(err.errors)
            }

            throw err
        })
    )
