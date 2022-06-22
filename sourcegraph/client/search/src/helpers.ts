import * as H from 'history'

import { ActivationProps } from '@sourcegraph/shared/src/components/activation/Activation'
import { CharacterRange } from '@sourcegraph/shared/src/search/query/token'

import { CaseSensitivityProps, SearchContextProps, SearchPatternTypeProps } from '.'

export enum QueryChangeSource {
    /**
     * When the user has typed in the query or selected a suggestion.
     * Prevents fetching/showing suggestions on every component update.
     */
    userInput,
}

/**
 * These hints instruct the editor to perform certain actions alongside updating
 * its value.
 */
export enum EditorHint {
    Focus = 1,
    /**
     * Showing suggestions also implies focusing the input.
     */
    ShowSuggestions = 2 | Focus,
}

/**
 * The search query and additional information depending on how the query was
 * changed. See MonacoQueryInput for how this data is applied to the editor.
 */
export type QueryState =
    | {
          changeSource: QueryChangeSource.userInput
          query: string
      }
    | {
          changeSource?: undefined
          query: string
          /** A bit field to instruct the editor to perform this action in
           * response to updating its state
           */
          hint?: EditorHint
          /** The query input will apply this selection */
          selectionRange?: CharacterRange
          /** Can be used to ensure that newly added or updated filters are
           * completely visible in the query input. */
          revealRange?: CharacterRange
      }

export interface SubmitSearchParameters
    extends Partial<Pick<ActivationProps, 'activation'>>,
        SearchPatternTypeProps,
        Pick<CaseSensitivityProps, 'caseSensitive'>,
        Pick<SearchContextProps, 'selectedSearchContextSpec'> {
    history: H.History
    query: string
    source:
        | 'home'
        | 'nav'
        | 'repo'
        | 'tree'
        | 'filter'
        | 'type'
        | 'scopePage'
        | 'communitySearchContextPage'
        | 'excludedResults'
    searchParameters?: { key: string; value: string }[]
}

export interface SubmitSearchProps {
    submitSearch: (parameters: Partial<Omit<SubmitSearchParameters, 'query'>>) => void
}

export function canSubmitSearch(query: string, selectedSearchContextSpec?: string): boolean {
    // A standalone context: filter is also a valid search query
    return query !== '' || !!selectedSearchContextSpec
}

/**
 * Returns the index that a given search scope occurs in a given search query.
 * Attempts to not match a scope that is a substring of another scope.
 *
 * @param query The full query string
 * @param scope A scope (sub query) that is searched for in `query`
 * @returns The index in `query`, or `-1` if not found
 */
export function queryIndexOfScope(query: string, scope: string): number {
    let index = 0
    while (true) {
        index = query.indexOf(scope, index)
        if (index === -1) {
            break
        }

        const boundAtStart = index === 0 || query[index - 1] === ' '
        const boundAtEnd = index + scope.length === query.length || query[index + scope.length] === ' '

        // prevent matching scopes that are substrings of other scopes
        if (!boundAtStart || !boundAtEnd) {
            index = index + 1
        } else {
            break
        }
    }
    return index
}

/**
 * Toggles the given search scope by adding or removing it from the current
 * user query string.
 *
 * @param query The current user query.
 * @param searchFilter The search scope (sub query) or dynamic filter to toggle (add/remove) from the current user query.
 * @returns The new query.
 */
export function toggleSubquery(query: string, searchFilter: string): string {
    const index = queryIndexOfScope(query, searchFilter)
    if (index === -1) {
        // Scope doesn't exist in search query, so add it now.
        return [query.trim(), searchFilter].filter(string => string).join(' ') + ' '
    }

    // Scope exists in the search query, so remove it now.
    return (query.slice(0, index).trim() + ' ' + query.slice(index + searchFilter.length).trim()).trim()
}
