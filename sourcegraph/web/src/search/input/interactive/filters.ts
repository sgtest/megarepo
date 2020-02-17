import { SuggestionTypes } from '../../../../../shared/src/search/suggestions/util'
import { Suggestion } from '../Suggestion'
import { assign } from 'lodash/fp'
import { FilterTypes } from '../../../../../shared/src/search/interactive/util'
import { getFilterDefinition } from '../../../../../shared/src/search/parser/filters'

/** FilterTypes which have a finite number of valid options. */
export type FiniteFilterTypes = FilterTypes.archived | FilterTypes.fork | FilterTypes.type

export const finiteFilters: Record<
    FiniteFilterTypes,
    {
        default: string
        values: Suggestion[]
    }
> = {
    archived: {
        default: 'yes',
        values: [{ value: 'no' }, { value: 'only' }, { value: 'yes' }].map(
            assign({
                type: SuggestionTypes.fork,
            })
        ),
    },
    fork: {
        default: 'yes',
        values: [{ value: 'no' }, { value: 'only' }, { value: 'yes' }].map(
            assign({
                type: SuggestionTypes.fork,
            })
        ),
    },
    type: {
        default: '',
        values: [
            { displayValue: 'code', value: '' },
            { value: 'commit' },
            { value: 'diff' },
            { value: 'repo' },
            { value: 'path' },
            { value: 'symbols' },
        ].map(
            assign({
                type: SuggestionTypes.type,
            })
        ),
    },
}

export const isFiniteFilter = (filter: FilterTypes): filter is FiniteFilterTypes =>
    !!getFilterDefinition(filter) && ['fork', 'archived', 'type'].includes(filter)

export function isTextFilter(filter: FilterTypes): boolean {
    return !!getFilterDefinition(filter) && !isFiniteFilter(filter)
}

/**
 * Some filter types should have their suggestions searched without influence
 * from the rest of the query, as they will then influence the scope of other filters.
 *
 * Same as {@link isolatedFuzzySearchFilters} but using FilterTypes rather than SuggestionTypes.
 */
export const isolatedFuzzySearchFiltersFilterType = [FilterTypes.repo, FilterTypes.repogroup]

export const FilterTypesToProseNames: Record<FilterTypes, string> = {
    repo: 'Repository',
    repogroup: 'Repository group',
    repohasfile: 'Repo has file',
    repohascommitafter: 'Repo has commit after',
    file: 'File',
    lang: 'Language',
    count: 'Count',
    timeout: 'Timeout',
    fork: 'Forks',
    archived: 'Archived repos',
    case: 'Case sensitive',
    after: 'Committed after',
    before: 'Committed before',
    message: 'Commit message contains',
    author: 'Commit author',
    type: 'Type',
    content: 'Content',
}
