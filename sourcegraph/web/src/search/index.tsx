import { escapeRegExp } from 'lodash'
import { SearchPatternType } from '../../../shared/src/graphql/schema'
import { FiltersToTypeAndValue } from '../../../shared/src/search/interactive/util'
import { parseCaseSensitivityFromQuery, parsePatternTypeFromQuery } from '../../../shared/src/util/url'
import { replaceRange } from '../../../shared/src/util/strings'
import { discreteValueAliases } from '../../../shared/src/search/parser/filters'

/**
 * Parses the query out of the URL search params (the 'q' parameter). In non-interactive mode, if the 'q' parameter is not present, it
 * returns undefined. When parsing for interactive mode, each filter's individual query parameter
 * will be parsed and detected.
 *
 * @param query the URL query parameters
 * @param interactiveMode whether to parse the search URL query in interactive mode, reading query params such as `repo=` and `file=`.
 * @param navbarQueryOnly whether to only parse the query for the main query input, i.e. only the value passed to the `q=`
 * URL query parameter, as this represents the query that appears in the main query input in both modes.
 *
 */
export function parseSearchURLQuery(query: string): string | undefined {
    const searchParams = new URLSearchParams(query)
    return searchParams.get('q') || undefined
}

/**
 * Parses the pattern type out of the URL search params (the 'patternType' parameter). If the 'pattern' parameter
 * is not present, or it is an invalid value, it returns undefined.
 */
export function parseSearchURLPatternType(query: string): SearchPatternType | undefined {
    const searchParams = new URLSearchParams(query)
    const patternType = searchParams.get('patternType')
    if (
        patternType !== SearchPatternType.literal &&
        patternType !== SearchPatternType.regexp &&
        patternType !== SearchPatternType.structural
    ) {
        return undefined
    }
    return patternType
}

export function searchURLIsCaseSensitive(query: string): boolean {
    const queryCaseSensitivity = parseCaseSensitivityFromQuery(query)
    if (queryCaseSensitivity) {
        // if `case:` filter exists in the query, override the existing case: query param
        return discreteValueAliases.yes.includes(queryCaseSensitivity.value)
    }
    const searchParams = new URLSearchParams(query)
    const caseSensitive = searchParams.get('case')
    return discreteValueAliases.yes.includes(caseSensitive || '')
}

/**
 * parseSearchURL takes a URL's search querystring and returns
 * an object containing:
 * - the canonical, user-visible query (with `patternType` and `case` filters excluded),
 * - the effective pattern type, and
 * - the effective case sensitivity of the query.
 *
 * @param urlSearchQuery a URL's query string.
 */
export function parseSearchURL(
    urlSearchQuery: string
): { query: string | undefined; patternType: SearchPatternType | undefined; caseSensitive: boolean } {
    let finalQuery = parseSearchURLQuery(urlSearchQuery) || ''
    let patternType = parseSearchURLPatternType(urlSearchQuery)
    let caseSensitive = searchURLIsCaseSensitive(urlSearchQuery)

    const patternTypeInQuery = parsePatternTypeFromQuery(finalQuery)
    if (patternTypeInQuery) {
        // Any `patterntype:` filter in the query should override the patternType= URL query parameter if it exists.
        finalQuery = replaceRange(finalQuery, patternTypeInQuery.range)
        patternType = patternTypeInQuery.value as SearchPatternType
    }

    const caseInQuery = parseCaseSensitivityFromQuery(finalQuery)
    if (caseInQuery) {
        // Any `case:` filter in the query should override the case= URL query parameter if it exists.
        finalQuery = replaceRange(finalQuery, caseInQuery.range)

        if (discreteValueAliases.yes.includes(caseInQuery.value)) {
            caseSensitive = true
        } else if (discreteValueAliases.no.includes(caseInQuery.value)) {
            caseSensitive = false
        }
    }

    return { query: finalQuery, patternType, caseSensitive }
}

export function repoFilterForRepoRev(repoName: string, rev?: string): string {
    return `${quoteIfNeeded(`^${escapeRegExp(repoName)}$${rev ? `@${abbreviateOID(rev)}` : ''}`)}`
}

export function searchQueryForRepoRev(repoName: string, rev?: string): string {
    return `repo:${repoFilterForRepoRev(repoName, rev)} `
}

function abbreviateOID(oid: string): string {
    if (oid.length === 40) {
        return oid.slice(0, 7)
    }
    return oid
}

export function quoteIfNeeded(s: string): string {
    if (/["' ]/.test(s)) {
        return JSON.stringify(s)
    }
    return s
}

export interface PatternTypeProps {
    patternType: SearchPatternType
    setPatternType: (patternType: SearchPatternType) => void
}

export interface CaseSensitivityProps {
    caseSensitive: boolean
    setCaseSensitivity: (caseSensitive: boolean) => void
}

export interface InteractiveSearchProps {
    filtersInQuery: FiltersToTypeAndValue
    onFiltersInQueryChange: (filtersInQuery: FiltersToTypeAndValue) => void
    splitSearchModes: boolean
    interactiveSearchMode: boolean
    toggleSearchMode: (event: React.MouseEvent<HTMLAnchorElement>) => void
}

export interface SmartSearchFieldProps {
    smartSearchField: boolean
}
