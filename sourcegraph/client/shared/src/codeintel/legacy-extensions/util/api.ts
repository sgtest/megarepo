import { once } from 'lodash'
import gql from 'tagged-template-noop'

import { searchContext } from '../../searchContext'
import type * as sourcegraph from '../api'
import { cache } from '../util'

import { graphqlIdToRepoId, queryGraphQL } from './graphql'
import { isDefined } from './helpers'
import { parseGitURI } from './uri'

/**
 * A search result. Each result is for a particular repository and commit, but
 * may have many symbol or indexed/un-indexed search results.
 */
export interface SearchResult {
    repository: {
        name: string
    }
    file: {
        path: string
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
        resource: { path: string }
        range?: sourcegraph.Range
    }
}

/**
 * An indexed or un-indexed search result.
 */
export interface LineMatch {
    lineNumber: number
    offsetAndLengths: [number, number][]
}

/** Metadata about a resolved repository. */
export interface RepoMeta {
    id: number
    name: string
    isFork: boolean
    isArchived: boolean
}

export class API {
    /**
     * Small never-evict map from repo names to a promise of their meta.
     *
     * We store a promise so that we can cache two requests to the same repo
     * before the request resolves
     */
    private cachedMetaRequests = new Map<string, Promise<RepoMeta>>()

    /**
     * Retrieves the name and fork/archive status of a repository. This method
     * throws an error if the repository is not known to the Sourcegraph instance.
     *
     * @param name The repository's name.
     */
    public async resolveRepo(name: string): Promise<RepoMeta> {
        const cachedMetaRequest = this.cachedMetaRequests.get(name)
        if (cachedMetaRequest !== undefined) {
            return cachedMetaRequest
        }

        const metaRequest = (async (name: string): Promise<RepoMeta> => {
            const queryWithFork = gql`
                query LegacyResolveRepo($name: String!) {
                    repository(name: $name) {
                        id
                        name
                        isFork
                        isArchived
                    }
                }
            `

            const queryWithoutFork = gql`
                query LegacyResolveRepo2($name: String!) {
                    repository(name: $name) {
                        name
                    }
                }
            `

            interface Response {
                repository: {
                    id: string
                    name: string
                    isFork?: boolean
                    isArchived?: boolean
                }
            }

            const data = await queryGraphQL<Response>((await this.hasForkField()) ? queryWithFork : queryWithoutFork, {
                name,
            })

            // Assume repo is not a fork/archived for older instances
            return {
                isFork: false,
                isArchived: false,
                ...data.repository,
                id: graphqlIdToRepoId(data.repository.id),
            }
        })(name)
        this.cachedMetaRequests.set(name, metaRequest)

        return metaRequest
    }

    /**
     * Determines via introspection if the GraphQL API has isFork field on the Repository type.
     *
     * TODO(efritz) - Remove this when we no longer need to support pre-3.15 instances.
     */
    private async hasForkField(): Promise<boolean> {
        const introspectionQuery = gql`
            query LegacyRepositoryIntrospection {
                __type(name: "Repository") {
                    fields {
                        name
                    }
                }
            }
        `

        interface IntrospectionResponse {
            __type: { fields: { name: string }[] }
        }

        return (await queryGraphQL<IntrospectionResponse>(introspectionQuery)).__type.fields.some(
            field => field.name === 'isFork'
        )
    }

    /**
     * Determines via introspection if the GraphQL API has local code intelligence available
     *
     * TODO(chrismwendt) - Remove this when we no longer need to support versions without local code
     * intelligence
     */
    public hasLocalCodeIntelField = once(async () => {
        const introspectionQuery = gql`
            query LegacyLocalCodeIntelIntrospectionQuery {
                __type(name: "GitBlob") {
                    fields {
                        name
                    }
                }
            }
        `

        interface IntrospectionResponse {
            __type: { fields: { name: string }[] }
        }

        return (await queryGraphQL<IntrospectionResponse>(introspectionQuery)).__type.fields.some(
            field => field.name === 'localCodeIntel'
        )
    })

    /**
     * Determines via introspection if the GraphQL API has symbol info available
     *
     * TODO(chrismwendt) - Remove this when we no longer need to support versions without symbol info
     */
    public hasSymbolInfo = once(async () => {
        const introspectionQuery = gql`
            query LegacySymbolInfoIntrospectionQuery {
                __type(name: "GitBlob") {
                    fields {
                        name
                    }
                }
            }
        `

        interface IntrospectionResponse {
            __type: { fields: { name: string }[] }
        }

        return (await queryGraphQL<IntrospectionResponse>(introspectionQuery)).__type.fields.some(
            field => field.name === 'symbolInfo'
        )
    })

    /**
     * Determines via introspection if the GraphQL API has symbolInfo.range available
     *
     * TODO(chrismwendt) - Remove this when we no longer need to support versions without symbolInfo.range
     */
    public hasSymbolLocationRange = once(async () => {
        const introspectionQuery = gql`
            query LegacySymbolLocationRangeIntrospectionQuery {
                __type(name: "SymbolLocation") {
                    fields {
                        name
                    }
                }
            }
        `

        interface IntrospectionResponse {
            __type: { fields: { name: string }[] }
        }

        return (await queryGraphQL<IntrospectionResponse>(introspectionQuery)).__type.fields.some(
            field => field.name === 'range'
        )
    })

    public fetchLocalCodeIntelPayload = cache(
        async ({ repo, commit, path }: RepoCommitPath): Promise<LocalCodeIntelPayload | undefined> => {
            const vars = { repository: repo, commit, path }
            const response = await queryGraphQL<LocalCodeIntelResponse>(localCodeIntelQuery, vars)

            const payloadString = response?.repository?.commit?.blob?.localCodeIntel
            if (!payloadString) {
                return undefined
            }

            return JSON.parse(payloadString) as LocalCodeIntelPayload
        },
        { max: 10 }
    )

    public findLocalSymbol = async (
        document: sourcegraph.TextDocument,
        position: sourcegraph.Position
    ): Promise<LocalSymbol | undefined> => {
        if (!(await this.hasLocalCodeIntelField())) {
            return
        }

        const { repo, commit, path } = parseGitURI(new URL(document.uri))

        const payload = await this.fetchLocalCodeIntelPayload({ repo, commit, path })
        if (!payload) {
            return
        }

        for (const symbol of payload.symbols) {
            if (isInRange(position, symbol.def)) {
                return symbol
            }

            for (const reference of symbol.refs ?? []) {
                if (isInRange(position, reference)) {
                    return symbol
                }
            }
        }

        return undefined
    }

    public fetchSymbolInfo = async (
        document: sourcegraph.TextDocument,
        position: sourcegraph.Position
    ): Promise<SymbolInfoCanonical | undefined> => {
        if (!(await this.hasSymbolInfo())) {
            return
        }

        const query = (await this.hasSymbolLocationRange())
            ? symbolInfoDefinitionQueryWithRange
            : symbolInfoDefinitionQueryWithoutRange

        const { repo, commit, path } = parseGitURI(new URL(document.uri))

        const vars = { repository: repo, commit, path, line: position.line, character: position.character }
        const response = await queryGraphQL<SymbolInfoResponse>(query, vars)

        const symbolInfoFlexible = response?.repository?.commit?.blob?.symbolInfo ?? undefined
        if (!symbolInfoFlexible) {
            return undefined
        }
        return symbolInfoFlexibleToCanonical(symbolInfoFlexible)
    }

    /**
     * Get the content of a file. Throws an error if the repository is not known to
     * the Sourcegraph instance. Returns undefined if the input rev or the file is
     * not known to the Sourcegraph instance.
     *
     * @param repo The repository in which the file exists.
     * @param revision The revision in which the target version of the file exists.
     * @param path The path of the file.
     */
    public async getFileContent(repo: string, revision: string, path: string): Promise<string | undefined> {
        const query = gql`
            query LegacyFileContent($repo: String!, $rev: String!, $path: String!) {
                repository(name: $repo) {
                    commit(rev: $rev) {
                        file(path: $path) {
                            content
                        }
                    }
                }
            }
        `

        interface Response {
            repository: {
                commit?: {
                    file?: { content: string }
                }
            }
        }

        const data = await queryGraphQL<Response>(query, { repo, rev: revision, path })
        return data.repository.commit?.file?.content
    }

    /**
     * Perform a search.
     *
     * @param searchQuery The input to the search command.
     * @param fileLocal Set to false to not request this field, which is absent in older versions of Sourcegraph.
     */
    public async search(searchQuery: string, fileLocal = true): Promise<SearchResult[]> {
        const context = searchContext()
        const query = context ? `context:${context} ${searchQuery}` : searchQuery

        interface Response {
            search: {
                results: {
                    limitHit: boolean
                    results: (SearchResult | undefined)[]
                }
            }
        }

        const data = await queryGraphQL<Response>(buildSearchQuery(fileLocal), {
            query,
        })
        return data.search.results.results.filter(isDefined)
    }

    /**
     * Determines via introspection if the GraphQL API supports stencils
     *
     * TODO(chrismwendt) - Remove this when we no longer need to support Sourcegraph versions that don't
     * have stencil support
     */
    public async hasStencils(): Promise<boolean> {
        const introspectionQuery = gql`
            query LegacyStencilIntrospectionQuery {
                __type(name: "GitBlobLSIFData") {
                    fields {
                        name
                    }
                }
            }
        `

        interface IntrospectionResponse {
            __type: { fields: { name: string }[] }
        }

        return Boolean(
            (await queryGraphQL<IntrospectionResponse>(introspectionQuery)).__type?.fields.some(
                field => field.name === 'stencil'
            )
        )
    }
}

function buildSearchQuery(fileLocal: boolean): string {
    const searchResultsFragment = gql`
        fragment SearchResults on Search {
            results {
                __typename
                results {
                    ... on FileMatch {
                        __typename
                        file {
                            path
                            commit {
                                oid
                            }
                        }
                        repository {
                            name
                        }
                        symbols {
                            name
                            kind
                            location {
                                resource {
                                    path
                                }
                                range {
                                    start {
                                        line
                                        character
                                    }
                                    end {
                                        line
                                        character
                                    }
                                }
                            }
                        }
                        lineMatches {
                            lineNumber
                            offsetAndLengths
                        }
                    }
                }
            }
        }
    `

    const fileLocalFragment = gql`
        fragment FileLocal on Search {
            results {
                __typename
                results {
                    ... on FileMatch {
                        symbols {
                            fileLocal
                        }
                    }
                }
            }
        }
    `

    if (fileLocal) {
        return gql`
            query LegacyCodeIntelSearch2($query: String!) {
                search(query: $query) {
                    ...SearchResults
                    ...FileLocal
                }
            }
            ${searchResultsFragment}
            ${fileLocalFragment}
        `
    }

    return gql`
        query LegacyCodeIntelSearch3($query: String!) {
            search(query: $query) {
                ...SearchResults
            }
        }
        ${searchResultsFragment}
    `
}

export interface RepoCommitPath {
    repo: string
    commit: string
    path: string
}

type LocalCodeIntelPayload = {
    symbols: LocalSymbol[]
} | null

interface LocalSymbol {
    hover?: string
    def: Range
    refs?: Range[]
}

export interface Range {
    row: number
    column: number
    length: number
}

const isInRange = (position: sourcegraph.Position, range: Range): boolean => {
    if (position.line !== range.row) {
        return false
    }
    if (position.character < range.column) {
        return false
    }
    if (position.character >= range.column + range.length) {
        return false
    }
    return true
}

/** The response envelope for all blob queries. */
interface GenericBlobResponse<R> {
    repository: { commit: { blob: R | null } | null } | null
}

type LocalCodeIntelResponse = GenericBlobResponse<{ localCodeIntel: string }>

const localCodeIntelQuery = gql`
    query LocalCodeIntel($repository: String!, $commit: String!, $path: String!) {
        repository(name: $repository) {
            id
            commit(rev: $commit) {
                blob(path: $path) {
                    localCodeIntel
                }
            }
        }
    }
`

type SymbolInfoResponse = GenericBlobResponse<{
    symbolInfo: SymbolInfoFlexible | null
}>

interface LineCharLength {
    line: number
    character: number
    length: number
}

interface SymbolInfoFlexible {
    definition: RepoCommitPath & (LineCharLength | { range?: LineCharLength })
    hover: string | null
}

interface SymbolInfoCanonical {
    definition: RepoCommitPath & { range?: LineCharLength }
    hover: string | null
}

const symbolInfoFlexibleToCanonical = (flexible: SymbolInfoFlexible): SymbolInfoCanonical => ({
    definition: {
        ...flexible.definition,
        range:
            'line' in flexible.definition
                ? {
                      line: flexible.definition.line,
                      character: flexible.definition.character,
                      length: flexible.definition.length,
                  }
                : flexible.definition.range,
    },
    hover: flexible.hover,
})

const symbolInfoDefinitionQueryWithoutRange = gql`
    query LegacySymbolInfo($repository: String!, $commit: String!, $path: String!, $line: Int!, $character: Int!) {
        repository(name: $repository) {
            commit(rev: $commit) {
                blob(path: $path) {
                    symbolInfo(line: $line, character: $character) {
                        definition {
                            repo
                            commit
                            path
                            line
                            character
                            length
                        }
                        hover
                    }
                }
            }
        }
    }
`

const symbolInfoDefinitionQueryWithRange = gql`
    query LegacySymbolInfo2($repository: String!, $commit: String!, $path: String!, $line: Int!, $character: Int!) {
        repository(name: $repository) {
            commit(rev: $commit) {
                blob(path: $path) {
                    symbolInfo(line: $line, character: $character) {
                        definition {
                            repo
                            commit
                            path
                            range {
                                line
                                character
                                length
                            }
                        }
                        hover
                    }
                }
            }
        }
    }
`
