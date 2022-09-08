import { replaceRange } from '@sourcegraph/common'

import { FILTERS, FilterType } from './filters'
import { findFilters, findFilter, FilterKind } from './query'
import { scanSearchQuery } from './scanner'
import { Filter, Token } from './token'
import { operatorExists, filterExists } from './validate'

export function appendContextFilter(query: string, searchContextSpec: string | undefined): string {
    return !filterExists(query, FilterType.context) && searchContextSpec
        ? `context:${searchContextSpec} ${query}`
        : query
}

/**
 * Deletes the filter from a given query string by the filter's range.
 */
export function omitFilter(query: string, filter: Filter): string {
    const { start, end } = filter.range

    return `${query.slice(0, start).trimEnd()} ${query.slice(end).trimStart()}`.trim()
}

const succeedScan = (query: string): Token[] => {
    const result = scanSearchQuery(query)
    if (result.type !== 'success') {
        throw new Error('Internal error: invariant broken: succeedScan callers must be called with a valid query')
    }
    return result.term
}

/**
 * Updates the first filter with the given value if it exists.
 * Appends a single filter at the top level of the query if it does not exist.
 * This function expects a valid query; if it is invalid it throws.
 */
export const updateFilter = (query: string, field: string, value: string): string => {
    const filters = findFilters(succeedScan(query), field)
    return filters.length > 0
        ? replaceRange(query, filters[0].range, `${field}:${value}`).trim()
        : `${query} ${field}:${value}`
}

/**
 * Updates all filters with the given value if they exist.
 * Appends a single filter at the top level of the query if none exist.
 * This function expects a valid query; if it is invalid it throws.
 */
export const updateFilters = (query: string, field: string, value: string): string => {
    const filters = findFilters(succeedScan(query), field)
    let modified = false
    for (const filter of filters.reverse()) {
        query = replaceRange(query, filter.range, `${field}:${value}`)
        modified = true
    }
    if (modified) {
        return query.trim()
    }
    return `${query} ${field}:${value}`
}

/**
 * Appends the provided filter.
 */
export const appendFilter = (query: string, field: string, value: string): string => {
    const trimmedQuery = query.trim()
    const filter = `${field}:${value}`
    return trimmedQuery.length === 0 ? filter : `${query.trimEnd()} ${filter}`
}

/**
 * Removes certain filters from a given query for privacy purposes, so query can be logged in telemtry.
 */
export const sanitizeQueryForTelemetry = (query: string): string => {
    const redactedValue = '[REDACTED]'
    const filterToRedact = [
        FilterType.repo,
        FilterType.file,
        FilterType.rev,
        FilterType.repohasfile,
        FilterType.context,
        FilterType.message,
    ]

    let newQuery = query

    for (const filter of filterToRedact) {
        if (filterExists(query, filter)) {
            newQuery = updateFilters(newQuery, filter, redactedValue)
        }
        if (filterExists(query, filter, true)) {
            newQuery = updateFilters(newQuery, `-${filter}`, redactedValue)
        }
        const alias = FILTERS[filter].alias
        if (alias) {
            if (filterExists(query, alias)) {
                newQuery = updateFilters(newQuery, alias, redactedValue)
            }
            if (filterExists(query, alias, true)) {
                newQuery = updateFilters(newQuery, `-${alias}`, redactedValue)
            }
        }
    }

    return newQuery
}

/**
 * Wraps a query in parenthesis if a global search context filter exists.
 * Example: context:ctx a or b -> context:ctx (a or b)
 */
export function parenthesizeQueryWithGlobalContext(query: string): string {
    if (!operatorExists(query)) {
        // no need to parenthesize a flat, atomic query.
        return query
    }
    const globalContextFilter = findFilter(query, FilterType.context, FilterKind.Global)
    if (!globalContextFilter) {
        // don't parenthesize a query that contains `context` subexpressions already.
        return query
    }
    const searchContextSpec = globalContextFilter.value?.value || ''
    const queryWithOmittedContext = omitFilter(query, globalContextFilter)
    return appendContextFilter(`(${queryWithOmittedContext})`, searchContextSpec)
}
