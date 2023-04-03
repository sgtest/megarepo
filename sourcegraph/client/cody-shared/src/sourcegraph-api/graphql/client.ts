import fetch from 'isomorphic-fetch'

import { isError } from '../../utils'

import {
    CURRENT_USER_ID_QUERY,
    IS_CONTEXT_REQUIRED_QUERY,
    REPOSITORY_ID_QUERY,
    SEARCH_EMBEDDINGS_QUERY,
    LOG_EVENT_MUTATION,
} from './queries'

interface APIResponse<T> {
    data?: T
    errors?: { message: string; path?: string[] }[]
}

interface CurrentUserIdResponse {
    currentUser: { id: string } | null
}

interface RepositoryIdResponse {
    repository: { id: string } | null
}

interface EmbeddingsSearchResponse {
    embeddingsSearch: EmbeddingsSearchResults
}

interface LogEventResponse {}

export interface EmbeddingsSearchResult {
    fileName: string
    startLine: number
    endLine: number
    content: string
}

export interface EmbeddingsSearchResults {
    codeResults: EmbeddingsSearchResult[]
    textResults: EmbeddingsSearchResult[]
}

interface IsContextRequiredForChatQueryResponse {
    isContextRequiredForChatQuery: boolean
}

function extractDataOrError<T, R>(response: APIResponse<T> | Error, extract: (data: T) => R): R | Error {
    if (isError(response)) {
        return response
    }
    if (response.errors && response.errors.length > 0) {
        return new Error(response.errors.map(({ message }) => message).join(', '))
    }
    if (!response.data) {
        return new Error('response is missing data')
    }
    return extract(response.data)
}

export class SourcegraphGraphQLAPIClient {
    private dotcomUrl = 'https://sourcegraph.com'

    constructor(private instanceUrl: string, private accessToken: string | null) {}

    public async getCurrentUserId(): Promise<string | Error> {
        return this.fetchSourcegraphAPI<APIResponse<CurrentUserIdResponse>>(CURRENT_USER_ID_QUERY, {}).then(response =>
            extractDataOrError(response, data =>
                data.currentUser ? data.currentUser.id : new Error('current user not found')
            )
        )
    }

    public async getRepoId(repoName: string): Promise<string | Error> {
        return this.fetchSourcegraphAPI<APIResponse<RepositoryIdResponse>>(REPOSITORY_ID_QUERY, {
            name: repoName,
        }).then(response =>
            extractDataOrError(response, data =>
                data.repository ? data.repository.id : new Error(`repository ${repoName} not found`)
            )
        )
    }

    public async logEvent(event: {
        event: string
        userCookieID: string
        url: string
        source: string
        argument?: string | {}
        publicArgument?: string | {}
    }): Promise<void | Error> {
        try {
            if (this.instanceUrl === this.dotcomUrl) {
                await this.fetchSourcegraphAPI<APIResponse<LogEventResponse>>(LOG_EVENT_MUTATION, event).then(
                    response => {
                        extractDataOrError(response, data => {})
                    }
                )
            } else {
                await Promise.all([
                    this.fetchSourcegraphAPI<APIResponse<LogEventResponse>>(LOG_EVENT_MUTATION, event).then(
                        response => {
                            extractDataOrError(response, data => {})
                        }
                    ),
                    this.fetchSourcegraphDotcomAPI<APIResponse<LogEventResponse>>(LOG_EVENT_MUTATION, event).then(
                        response => {
                            extractDataOrError(response, data => {})
                        }
                    ),
                ])
            }
        } catch (error) {
            return error
        }
    }

    public async searchEmbeddings(
        repo: string,
        query: string,
        codeResultsCount: number,
        textResultsCount: number
    ): Promise<EmbeddingsSearchResults | Error> {
        return this.fetchSourcegraphAPI<APIResponse<EmbeddingsSearchResponse>>(SEARCH_EMBEDDINGS_QUERY, {
            repo,
            query,
            codeResultsCount,
            textResultsCount,
        }).then(response => extractDataOrError(response, data => data.embeddingsSearch))
    }

    public async isContextRequiredForQuery(query: string): Promise<boolean | Error> {
        return this.fetchSourcegraphAPI<APIResponse<IsContextRequiredForChatQueryResponse>>(IS_CONTEXT_REQUIRED_QUERY, {
            query,
        }).then(response => extractDataOrError(response, data => data.isContextRequiredForChatQuery))
    }

    private fetchSourcegraphAPI<T>(query: string, variables: Record<string, any>): Promise<T | Error> {
        return fetch(`${this.instanceUrl}/.api/graphql`, {
            headers: { ...(this.accessToken ? { Authorization: `token ${this.accessToken}` } : null) },
            method: 'POST',
            body: JSON.stringify({ query, variables }),
        })
            .then(verifyResponseCode)
            .then(response => response.json() as T)
            .catch(error => new Error(`accessing Sourcegraph GraphQL API: ${error}`))
    }

    // make an anonymous request to the dotcom API
    private async fetchSourcegraphDotcomAPI<T>(query: string, variables: Record<string, any>): Promise<T | Error> {
        return fetch(`${this.dotcomUrl}/.api/graphql`, {
            method: 'POST',
            body: JSON.stringify({ query, variables }),
        })
            .then(verifyResponseCode)
            .then(response => response.json() as T)
            .catch(() => new Error('error fetching Sourcegraph GraphQL API'))
    }
}

function verifyResponseCode(response: Response): Response {
    if (!response.ok) {
        throw new Error(`HTTP status code: ${response.status}`)
    }
    return response
}
