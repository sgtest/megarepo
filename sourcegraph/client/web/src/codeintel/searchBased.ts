import { extname } from 'path'

import escapeRegExp from 'lodash/escapeRegExp'

import { appendLineRangeQueryParameter, toPositionOrRangeQueryParameter } from '@sourcegraph/common'
import { Range } from '@sourcegraph/extension-api-types'

import { LanguageSpec } from './language-specs/languagespec'
import { raceWithDelayOffset } from './promise'
import { SettingsGetter } from './settings'
import { isDefined } from './util/helpers'

export function definitionQuery({
    searchToken,
    path,
    fileExts,
}: {
    /** The search token text. */
    searchToken: string
    /** The path to file **/
    path: string
    /** File extensions used by the current extension. */
    fileExts: string[]
}): string[] {
    return [
        `^${searchToken}$`,
        'type:symbol',
        'patternType:regexp',
        'count:500',
        'case:yes',
        fileExtensionTerm(path, fileExts),
    ]
}

/**
 * Create a search query to find references of a symbol.
 *
 * @param args Parameter bag.
 */
export function referencesQuery({
    searchToken,
    path,
    fileExts,
}: {
    /** The search token text. */
    searchToken: string
    /** The path to file **/
    path: string
    /** File extensions used by the current extension. */
    fileExts: string[]
}): string[] {
    let pattern = ''
    if (/^\w/.test(searchToken)) {
        pattern += '\\b'
    }
    pattern += escapeRegExp(searchToken)
    if (/\w$/.test(searchToken)) {
        pattern += '\\b'
    }
    return [pattern, 'type:file', 'patternType:regexp', 'count:500', 'case:yes', fileExtensionTerm(path, fileExts)]
}
/**
 * Constructs a file term containing include-listed extensions. If the current
 * text document path has an excluded extension or an extension absent from the
 * include list, an empty file term will be returned.
 *
 * @param textDocument The current text document.
 * @param includelist The file extensions for the current language.
 */
function fileExtensionTerm(path: string, includelist: string[]): string {
    const extension = extname(path).slice(1)
    if (!extension || excludelist.has(extension) || !includelist.includes(extension)) {
        return ''
    }

    return `file:\\.(${includelist.join('|')})$`
}

const excludelist = new Set(['thrift', 'proto', 'graphql'])

/**
 * Returns fork and archived terms that should be supplied with the query.
 *
 * @param includeFork Whether or not the include forked repositories regardless of settings.
 * @param includeArchived Whether or not the include archived repositories regardless of settings.
 * @param getSetting Used to query user settings for code intel configuration.
 */
export function repositoryKindTerms(
    includeFork: boolean,
    includeArchived: boolean,
    getSetting: SettingsGetter
): string[] {
    const additionalTerms = []
    if (includeFork || getSetting('basicCodeIntel.includeForks', false)) {
        additionalTerms.push('fork:yes')
    }

    if (includeArchived || getSetting('basicCodeIntel.includeArchives', false)) {
        additionalTerms.push('archived:yes')
    }

    return additionalTerms
}
/** Returns a regular expression matching the given repository. */
function makeRepositoryPattern(repo: string): string {
    return `^${repo.replace(/ /g, '\\ ')}$`
}

/** The time in ms to delay between unindexed search request and the fallback indexed search request. */
const DEFAULT_UNINDEXED_SEARCH_TIMEOUT_MS = 5000

/**
 * Invoke the given search function by modifying the query with a term that will
 * only look in the current git tree by appending a repo filter with the repo name
 * and the current commit or, if `negateRepoFilter` is set, outside of current git
 * tree.
 *
 * This is likely to timeout on large repos or organizations with monorepos if the
 * current commit is not an indexed commit. Instead of waiting for a timeout, we
 * will start a second index-only search of the HEAD commit for the same repo after
 * a short delay.
 *
 * This function returns the set of results that resolve first.
 *
 * @param search The search function.
 * @param args The arguments to the search function.
 * @param negateRepoFilter Whether to look only inside or outside the given repo.
 */
export function searchWithFallback<
    P extends {
        repo: string
        isFork: boolean
        isArchived: boolean
        commit: string
        queryTerms: string[]
    },
    R
>(search: (args: P) => Promise<R>, args: P, negateRepoFilter = false, getSetting: SettingsGetter): Promise<R> {
    if (getSetting<boolean>('basicCodeIntel.indexOnly', false)) {
        return searchIndexed(search, args, negateRepoFilter, getSetting)
    }

    return raceWithDelayOffset(
        searchUnindexed(search, args, negateRepoFilter, getSetting),
        () => searchIndexed(search, args, negateRepoFilter, getSetting),
        getSetting<number>('basicCodeIntel.unindexedSearchTimeout', DEFAULT_UNINDEXED_SEARCH_TIMEOUT_MS)
    )
}

/**
 * Invoke the given search function as an indexed-only (fast, imprecise) search.
 *
 * @param search The search function.
 * @param args The arguments to the search function.
 * @param negateRepoFilter Whether to look only inside or outside the given repo.
 */
function searchIndexed<
    P extends {
        repo: string
        isFork: boolean
        isArchived: boolean
        commit: string
        queryTerms: string[]
    },
    R
>(search: (args: P) => Promise<R>, args: P, negateRepoFilter = false, getSetting: SettingsGetter): Promise<R> {
    const { repo, isFork, isArchived, queryTerms } = args

    // Create a copy of the args so that concurrent calls to other
    // search methods do not have their query terms unintentionally
    // modified.
    const queryTermsCopy = [...queryTerms]

    // Unlike unindexed search, we can't supply a commit as that particular
    // commit may not be indexed. We force index and look inside/outside
    // the repo at _whatever_ commit happens to be indexed at the time.
    queryTermsCopy.push((negateRepoFilter ? '-' : '') + `repo:${makeRepositoryPattern(repo)}`)
    queryTermsCopy.push('index:only')

    // If we're a fork, search in forks _for the same repo_. Otherwise,
    // search in forks only if it's set in the settings. This is also
    // symmetric for archived repositories.
    queryTermsCopy.push(
        ...repositoryKindTerms(isFork && !negateRepoFilter, isArchived && !negateRepoFilter, getSetting)
    )

    return search({ ...args, queryTerms: queryTermsCopy })
}

/**
 * Invoke the given search function as an unindexed (slow, precise) search.
 *
 * @param search The search function.
 * @param args The arguments to the search function.
 * @param negateRepoFilter Whether to look only inside or outside the given repo.
 */
function searchUnindexed<
    P extends {
        repo: string
        isFork: boolean
        isArchived: boolean
        commit: string
        queryTerms: string[]
    },
    R
>(search: (args: P) => Promise<R>, args: P, negateRepoFilter = false, getSetting: SettingsGetter): Promise<R> {
    const { repo, isFork, isArchived, commit, queryTerms } = args

    // Create a copy of the args so that concurrent calls to other
    // search methods do not have their query terms unintentionally
    // modified.
    const queryTermsCopy = [...queryTerms]

    if (!negateRepoFilter) {
        // Look in this commit only
        queryTermsCopy.push(`repo:${makeRepositoryPattern(repo)}@${commit}`)
    } else {
        // Look outside the repo (not outside the commit)
        queryTermsCopy.push(`-repo:${makeRepositoryPattern(repo)}`)
    }

    // If we're a fork, search in forks _for the same repo_. Otherwise,
    // search in forks only if it's set in the settings. This is also
    // symmetric for archived repositories.
    queryTermsCopy.push(
        ...repositoryKindTerms(isFork && !negateRepoFilter, isArchived && !negateRepoFilter, getSetting)
    )

    return search({ ...args, queryTerms: queryTermsCopy })
}

export function isSourcegraphDotCom(): boolean {
    return window.context?.sourcegraphDotComMode
}

/**
 * Report whether the given symbol is both private and does not belong to
 * the current text document.
 *
 * @param textDocument The current text document.
 * @param path The path of the document.
 * @param result The search result.
 */
export function isExternalPrivateSymbol(
    spec: LanguageSpec,
    path: string,
    { fileLocal, file, symbolKind }: Result
): boolean {
    // Enum members are always public, but there's an open ctags bug that
    // doesn't let us treat that way.
    // See https://github.com/universal-ctags/ctags/issues/1844

    if (spec.languageID === 'java' && symbolKind === 'ENUMMEMBER') {
        return false
    }

    return !!fileLocal && file !== path
}

export interface SearchResult {
    repository: {
        name: string
    }
    file: {
        url: string
        path: string
        content: string
        commit: {
            oid: string
        }
    }
    symbols: SearchSymbol[]
    lineMatches: LineMatch[]
}

/**
 * A symbol search result.
 */
export interface SearchSymbol {
    name: string
    fileLocal: boolean
    kind: string
    location: {
        url: string
        resource: { path: string }
        range?: Range
    }
}

/**
 * An indexed or un-indexed search result.
 */
export interface LineMatch {
    lineNumber: number
    offsetAndLengths: [number, number][]
}

export interface Result {
    /** The name of the repository containing the result. */
    repo: string

    /** The commit containing the result. */
    rev: string

    /** The path to the result file relative to the repository root. */
    file: string

    /** The content of the file. */
    content: string

    /** The unique URL to this result. */

    url: string

    /** The range of the match. */
    range: Range

    /** The type of symbol, if the result came from a symbol search. */
    symbolKind?: string

    /**
     * Whether or not the symbol is local to the containing file, if
     * the result came from a symbol search.
     */
    fileLocal?: boolean
}

/**
 * Convert a search result into a set of results.
 *
 * @param searchResult The search result.
 */
export function searchResultToResults({ ...result }: SearchResult): Result[] {
    const symbolResults = result.symbols
        ? result.symbols.map(symbol => searchResultSymbolToResults(result, symbol))
        : []

    const lineMatchResults = result.lineMatches
        ? result.lineMatches.flatMap(matches => lineMatchesToResults(result, matches))
        : []

    return symbolResults.filter(isDefined).concat(lineMatchResults)
}

/**
 * Convert a search symbol to a result.
 *
 * @param arg0 The parent search result.
 * @param arg1 The search symbol.
 */
function searchResultSymbolToResults(
    {
        repository: { name: repo },
        file: {
            content,
            commit: { oid: revision },
        },
    }: SearchResult,
    {
        kind: symbolKind,
        fileLocal,
        location: {
            url,
            resource: { path: file },
            range,
        },
    }: SearchSymbol
): Result | undefined {
    return (
        range && {
            repo,
            rev: revision,
            file,
            range,
            symbolKind,
            fileLocal,
            url,
            content,
        }
    )
}

/**
 * Convert a line match to a result.
 *
 * @param arg0 The parent search result.
 * @param arg1 The line match.
 */
function lineMatchesToResults(
    {
        repository: { name: repo },
        file: {
            content,
            url: fileUrl,
            path: file,
            commit: { oid: revision },
        },
    }: SearchResult,
    { lineNumber, offsetAndLengths }: LineMatch
): Result[] {
    return offsetAndLengths.map(([offset, length]) => {
        const url = appendLineRangeQueryParameter(
            fileUrl,
            toPositionOrRangeQueryParameter({
                position: { line: lineNumber + 1, character: offset + 1 },
            })
        )
        return {
            repo,
            rev: revision,
            file,
            url,
            range: {
                start: {
                    line: lineNumber,
                    character: offset,
                },
                end: {
                    line: lineNumber,
                    character: offset + length,
                },
            },
            content,
        }
    })
}
