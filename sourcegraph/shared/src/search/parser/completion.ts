import * as Monaco from 'monaco-editor'
import { escapeRegExp, startCase } from 'lodash'
import { FILTERS, resolveFilter } from './filters'
import { Sequence, toMonacoRange } from './parser'
import { Omit } from 'utility-types'
import { Observable } from 'rxjs'
import { SearchSuggestion, IRepository, IFile, ISymbol, ILanguage, SymbolKind } from '../../graphql/schema'
import { isDefined } from '../../util/types'
import { FilterTypes, isNegatableFilter } from '../interactive/util'

const repositoryCompletionItemKind = Monaco.languages.CompletionItemKind.Color
const filterCompletionItemKind = Monaco.languages.CompletionItemKind.Customcolor

type PartialCompletionItem = Omit<Monaco.languages.CompletionItem, 'range'>

const FILTER_TYPE_COMPLETIONS: Omit<Monaco.languages.CompletionItem, 'range'>[] = Object.keys(FILTERS)
    .flatMap(label => {
        const filterType = label as FilterTypes
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

const repositoryToCompletion = ({ name }: IRepository): PartialCompletionItem => ({
    label: name,
    kind: repositoryCompletionItemKind,
    insertText: `^${escapeRegExp(name)}$ `,
    filterText: name,
})

const fileToCompletion = ({ name, path, repository, isDirectory }: IFile): PartialCompletionItem => ({
    label: name,
    kind: isDirectory ? Monaco.languages.CompletionItemKind.Folder : Monaco.languages.CompletionItemKind.File,
    insertText: `^${escapeRegExp(path)}$ `,
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

const suggestionToCompletionItem = (suggestion: SearchSuggestion): PartialCompletionItem | undefined => {
    switch (suggestion.__typename) {
        case 'File':
            return fileToCompletion(suggestion)
        case 'Repository':
            return repositoryToCompletion(suggestion)
        case 'Symbol':
            return symbolToCompletion(suggestion)
        case 'Language':
            return languageToCompletion(suggestion)
    }
}

/**
 * Returns the completion items for a search query being typed in the Monaco query input,
 * including both static and dynamically fetched suggestions.
 */
export async function getCompletionItems(
    rawQuery: string,
    { members }: Pick<Sequence, 'members'>,
    { column }: Pick<Monaco.Position, 'column'>,
    fetchSuggestions: (query: string) => Observable<SearchSuggestion[]>
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
            })
        )
        const dynamicSuggestions = (await fetchSuggestions(rawQuery).toPromise())
            .map(suggestionToCompletionItem)
            .filter(isDefined)
            .map(completionItem => ({
                ...completionItem,
                range: toMonacoRange(range),
                // Set a sortText so that dynamic suggestions
                // are shown after filter type suggestions.
                sortText: '1',
            }))
        return {
            suggestions: [...staticSuggestions, ...dynamicSuggestions],
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
            const suggestions = await fetchSuggestions(rawQuery).toPromise()
            return {
                suggestions: suggestions
                    .filter(({ __typename }) => __typename === resolvedFilter.definition.suggestions)
                    .map(suggestionToCompletionItem)
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
