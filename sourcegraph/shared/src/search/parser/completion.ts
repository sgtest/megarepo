import * as Monaco from 'monaco-editor'
import { escapeRegExp, startCase } from 'lodash'
import { FILTERS, resolveFilter } from './filters'
import { Sequence, toMonacoRange } from './parser'
import { Omit } from 'utility-types'
import { Observable } from 'rxjs'
import { IRepository, IFile, ISymbol, ILanguage, IRepoGroup, SymbolKind } from '../../graphql/schema'
import { SearchSuggestion } from '../suggestions'
import { isDefined } from '../../util/types'
import { FilterType, isNegatableFilter } from '../interactive/util'
import { first } from 'rxjs/operators'

export const repositoryCompletionItemKind = Monaco.languages.CompletionItemKind.Color
const filterCompletionItemKind = Monaco.languages.CompletionItemKind.Customcolor

type PartialCompletionItem = Omit<Monaco.languages.CompletionItem, 'range'>

const FILTER_TYPE_COMPLETIONS: Omit<Monaco.languages.CompletionItem, 'range'>[] = Object.keys(FILTERS)
    .flatMap(label => {
        const filterType = label as FilterType
        const completionItem: Omit<Monaco.languages.CompletionItem, 'range' | 'detail'> = {
            label,
            kind: filterCompletionItemKind,
            insertText: `${label}:`,
            filterText: label,
        }
        if (isNegatableFilter(filterType)) {
            return [
                {
                    ...completionItem,
                    detail: FILTERS[filterType].description(false),
                },
                {
                    ...completionItem,
                    label: `-${label}`,
                    filterText: `-${label}`,
                    detail: FILTERS[filterType].description(true),
                },
            ]
        }
        return [
            {
                ...completionItem,
                detail: FILTERS[filterType].description,
            },
        ]
    })
    // Set a sortText so that filter type suggestions
    // are shown before dynamic suggestions.
    .map((completionItem, idx) => ({
        ...completionItem,
        sortText: `0${idx}`,
    }))

const repositoryToCompletion = ({ name }: IRepository, options: { isFilterValue: boolean }): PartialCompletionItem => ({
    label: name,
    kind: repositoryCompletionItemKind,
    insertText: options.isFilterValue ? `^${escapeRegExp(name)}$ ` : `repo:^${escapeRegExp(name)}$ `,
    filterText: name,
    detail: options.isFilterValue ? undefined : 'Repository',
})

const fileToCompletion = (
    { name, path, repository, isDirectory }: IFile,
    options: { isFilterValue: boolean }
): PartialCompletionItem => ({
    label: name,
    kind: isDirectory ? Monaco.languages.CompletionItemKind.Folder : Monaco.languages.CompletionItemKind.File,
    insertText: options.isFilterValue ? `^${escapeRegExp(path)}$` : `file:^${escapeRegExp(name)}$ `,
    filterText: name,
    detail: `${path} - ${repository.name}`,
})

/**
 * Maps Sourcegraph SymbolKinds to Monaco CompletionItemKinds.
 */
const symbolKindToCompletionItemKind: Record<SymbolKind, Monaco.languages.CompletionItemKind> = {
    UNKNOWN: Monaco.languages.CompletionItemKind.Value,
    FILE: Monaco.languages.CompletionItemKind.File,
    MODULE: Monaco.languages.CompletionItemKind.Module,
    NAMESPACE: Monaco.languages.CompletionItemKind.Module,
    PACKAGE: Monaco.languages.CompletionItemKind.Module,
    CLASS: Monaco.languages.CompletionItemKind.Class,
    METHOD: Monaco.languages.CompletionItemKind.Method,
    PROPERTY: Monaco.languages.CompletionItemKind.Property,
    FIELD: Monaco.languages.CompletionItemKind.Field,
    CONSTRUCTOR: Monaco.languages.CompletionItemKind.Constructor,
    ENUM: Monaco.languages.CompletionItemKind.Enum,
    INTERFACE: Monaco.languages.CompletionItemKind.Interface,
    FUNCTION: Monaco.languages.CompletionItemKind.Function,
    VARIABLE: Monaco.languages.CompletionItemKind.Variable,
    CONSTANT: Monaco.languages.CompletionItemKind.Constant,
    STRING: Monaco.languages.CompletionItemKind.Value,
    NUMBER: Monaco.languages.CompletionItemKind.Value,
    BOOLEAN: Monaco.languages.CompletionItemKind.Value,
    ARRAY: Monaco.languages.CompletionItemKind.Value,
    OBJECT: Monaco.languages.CompletionItemKind.Value,
    KEY: Monaco.languages.CompletionItemKind.Property,
    NULL: Monaco.languages.CompletionItemKind.Value,
    ENUMMEMBER: Monaco.languages.CompletionItemKind.EnumMember,
    STRUCT: Monaco.languages.CompletionItemKind.Struct,
    EVENT: Monaco.languages.CompletionItemKind.Event,
    OPERATOR: Monaco.languages.CompletionItemKind.Operator,
    TYPEPARAMETER: Monaco.languages.CompletionItemKind.TypeParameter,
}

const symbolToCompletion = ({ name, kind, location }: ISymbol): PartialCompletionItem => ({
    label: name,
    kind: symbolKindToCompletionItemKind[kind],
    insertText: name,
    filterText: name,
    detail: `${startCase(kind.toLowerCase())} - ${location.resource.repository.name}`,
})

const languageToCompletion = ({ name }: ILanguage): PartialCompletionItem | undefined =>
    name
        ? {
              label: name,
              kind: Monaco.languages.CompletionItemKind.TypeParameter,
              insertText: name,
              filterText: name,
          }
        : undefined

const repoGroupToCompletion = ({ name }: IRepoGroup): PartialCompletionItem => ({
    label: name,
    kind: repositoryCompletionItemKind,
    insertText: name,
    filterText: name,
})

const suggestionToCompletionItem = (
    suggestion: SearchSuggestion,
    options: { isFilterValue: boolean }
): PartialCompletionItem | undefined => {
    switch (suggestion.__typename) {
        case 'File':
            return fileToCompletion(suggestion, options)
        case 'Repository':
            return repositoryToCompletion(suggestion, options)
        case 'Symbol':
            return symbolToCompletion(suggestion)
        case 'Language':
            return languageToCompletion(suggestion)
        case 'RepoGroup':
            return repoGroupToCompletion(suggestion)
    }
}

/**
 * An internal Monaco command causing completion providers to be invoked,
 * and the suggestions widget to be shown.
 *
 * Useful to show the suggestions widget right after selecting a filter type
 * completion, to offer filter values completions.
 */
const TRIGGER_SUGGESTIONS: Monaco.languages.Command = {
    id: 'editor.action.triggerSuggest',
    title: 'Trigger suggestions',
}

/**
 * Returns the completion items for a search query being typed in the Monaco query input,
 * including both static and dynamically fetched suggestions.
 */
export async function getCompletionItems(
    { members }: Pick<Sequence, 'members'>,
    { column }: Pick<Monaco.Position, 'column'>,
    dynamicSuggestions: Observable<SearchSuggestion[]>
): Promise<Monaco.languages.CompletionList | null> {
    const defaultRange = {
        startLineNumber: 1,
        endLineNumber: 1,
        startColumn: column,
        endColumn: column,
    }
    // Show all filter suggestions on the first column.
    if (column === 1) {
        return {
            suggestions: FILTER_TYPE_COMPLETIONS.map(
                (suggestion): Monaco.languages.CompletionItem => ({
                    ...suggestion,
                    range: defaultRange,
                    command: TRIGGER_SUGGESTIONS,
                })
            ),
        }
    }
    const tokenAtColumn = members.find(({ range }) => range.start + 1 <= column && range.end + 1 >= column)
    if (!tokenAtColumn) {
        throw new Error('getCompletionItems: no token at column')
    }
    const { token, range } = tokenAtColumn
    // When the token at column is a literal or whitespace, show
    // static filter type suggestions, followed by dynamic suggestions.
    if (token.type === 'literal' || token.type === 'whitespace') {
        // Offer autocompletion of filter values
        const staticSuggestions = FILTER_TYPE_COMPLETIONS.map(
            (suggestion): Monaco.languages.CompletionItem => ({
                ...suggestion,
                range: toMonacoRange(range),
                command: TRIGGER_SUGGESTIONS,
            })
        )
        // If the token being typed matches a known filter,
        // only return static filter type suggestions.
        // This avoids blocking on dynamic suggestions to display
        // the suggestions widget.
        if (
            token.type === 'literal' &&
            staticSuggestions.some(({ label }) => label.startsWith(token.value.toLowerCase()))
        ) {
            return { suggestions: staticSuggestions }
        }
        return {
            suggestions: [
                ...staticSuggestions,
                ...(await dynamicSuggestions.pipe(first()).toPromise())
                    .map(suggestion => suggestionToCompletionItem(suggestion, { isFilterValue: false }))
                    .filter(isDefined)
                    .map(completionItem => ({
                        ...completionItem,
                        range: toMonacoRange(range),
                        // Set a sortText so that dynamic suggestions
                        // are shown after filter type suggestions.
                        sortText: '1',
                    })),
            ],
        }
    }
    if (token.type === 'filter') {
        const { filterValue } = token
        const completingValue = !filterValue || filterValue.range.start + 1 <= column
        if (!completingValue) {
            return null
        }
        const resolvedFilter = resolveFilter(token.filterType.token.value)
        if (!resolvedFilter) {
            return null
        }
        if (resolvedFilter.definition.suggestions) {
            if (resolvedFilter.definition.suggestions instanceof Array) {
                return {
                    suggestions: resolvedFilter.definition.suggestions.map(label => ({
                        label,
                        kind: Monaco.languages.CompletionItemKind.Text,
                        insertText: label,
                        range: filterValue ? toMonacoRange(filterValue.range) : defaultRange,
                    })),
                }
            }
            // If the filter definition has an associated suggestion type,
            // use it to filter dynamic suggestions.
            const suggestions = await dynamicSuggestions.pipe(first()).toPromise()
            return {
                suggestions: suggestions
                    .filter(({ __typename }) => __typename === resolvedFilter.definition.suggestions)
                    .map(suggestion => suggestionToCompletionItem(suggestion, { isFilterValue: true }))
                    .filter(isDefined)
                    .map(partialCompletionItem => ({
                        ...partialCompletionItem,
                        range: filterValue ? toMonacoRange(filterValue.range) : defaultRange,
                    })),
            }
        }
        if (resolvedFilter.definition.discreteValues) {
            return {
                suggestions: resolvedFilter.definition.discreteValues.map(
                    (label): Monaco.languages.CompletionItem => ({
                        label,
                        kind: Monaco.languages.CompletionItemKind.Value,
                        insertText: `${label} `,
                        filterText: label,
                        range: filterValue ? toMonacoRange(filterValue.range) : defaultRange,
                    })
                ),
            }
        }
    }
    return null
}
