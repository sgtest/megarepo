import { QueryResult } from '@sourcegraph/extensions-client-common/lib/graphql'
import { IQuery } from '@sourcegraph/extensions-client-common/lib/schema/graphqlschema'
import { Observable, throwError } from 'rxjs'
import { ajax } from 'rxjs/ajax'
import { catchError, map, switchMap } from 'rxjs/operators'
import { GQL } from '../../types/gqlschema'
import { removeAccessToken } from '../auth/access_token'
import { DEFAULT_SOURCEGRAPH_URL, isPrivateRepository, repoUrlCache, sourcegraphUrl } from '../util/context'
import { RequestContext } from './context'
import { AuthRequiredError, createAuthRequiredError, PrivateRepoPublicSourcegraphComError } from './errors'
import { getHeaders } from './headers'

/**
 * Interface for the response result of a GraphQL mutation
 */
export interface MutationResult {
    data?: GQL.IMutation
    errors?: GQL.IGraphQLResponseError[]
}

export interface GraphQLRequestArgs {
    ctx: RequestContext
    request: string
    variables?: any
    url?: string
    retry?: boolean
    /**
     * Whether or not to use an access token for the request. All requests
     * except requests used while creating an access token  should use an access
     * token. i.e. `createAccessToken` and the `fetchCurrentUser` used to get the
     * user ID for `createAccessToken`.
     */
    useAccessToken?: boolean
    authError?: AuthRequiredError
    requestMightContainPrivateInfo?: boolean
}

/**
 * Does a GraphQL request to the Sourcegraph GraphQL API running under `/.api/graphql`
 *
 * @param request The GraphQL request (query or mutation)
 * @param variables A key/value object with variable values
 * @param url the url the request is going to
 * @param options configuration options for the request
 * @return Observable That emits the result or errors if the HTTP request failed
 */
function requestGraphQL<T extends GQL.IGraphQLResponseRoot>({
    ctx,
    request,
    variables = {},
    url = sourcegraphUrl,
    retry = true,
    useAccessToken = true,
    authError,
    requestMightContainPrivateInfo = true,
}: GraphQLRequestArgs): Observable<T> {
    const nameMatch = request.match(/^\s*(?:query|mutation)\s+(\w+)/)
    const queryName = nameMatch ? '?' + nameMatch[1] : ''

    // Check if it's a private repo - if so don't make a request to Sourcegraph.com.
    if (isPrivateRepository() && url === DEFAULT_SOURCEGRAPH_URL && requestMightContainPrivateInfo) {
        return throwError(new PrivateRepoPublicSourcegraphComError(nameMatch ? nameMatch[1] : '<unnamed>'))
    }

    return getHeaders(url, useAccessToken).pipe(
        switchMap(headers =>
            ajax({
                method: 'POST',
                url: `${url}/.api/graphql` + queryName,
                headers,
                crossDomain: true,
                withCredentials: !(headers && headers.authorization),
                body: JSON.stringify({ query: request, variables }),
                async: true,
            }).pipe(
                map(({ response }) => {
                    if (shouldResponseTriggerRetryOrError(response)) {
                        delete repoUrlCache[ctx.repoKey]
                        throw response
                    }
                    if (ctx.isRepoSpecific && response.data.repository) {
                        repoUrlCache[ctx.repoKey] = url
                    }
                    return response
                }),
                catchError(err => {
                    if (err.status === 401) {
                        // Ensure all urls are tried and update authError to be the last seen 401.
                        // This ensures that the correct URL is used for sign in and also that all possible
                        // urls were checked.
                        authError = createAuthRequiredError(url)

                        if (headers && headers.authorization) {
                            // If we got a 401 with a token, get rid of the and
                            // try again. The token may be invalid and we just
                            // need to recreate one.
                            return removeAccessToken(url).pipe(
                                switchMap(() =>
                                    requestGraphQL({
                                        ctx,
                                        request,
                                        variables,
                                        url,
                                        retry,
                                        useAccessToken,
                                        authError,
                                        requestMightContainPrivateInfo,
                                    })
                                )
                            )
                        }
                    }

                    if (!retry || url === DEFAULT_SOURCEGRAPH_URL) {
                        // If there was an auth error and we tried all of the possible URLs throw the auth error.
                        if (authError) {
                            throw authError
                        }
                        delete repoUrlCache[ctx.repoKey]
                        // We just tried the last url
                        throw err
                    }

                    return requestGraphQL({
                        ctx,
                        request,
                        variables,
                        url: DEFAULT_SOURCEGRAPH_URL,
                        retry,
                        useAccessToken: true,
                        authError,
                        requestMightContainPrivateInfo,
                    })
                })
            )
        )
    )
}

/**
 * Checks the GraphQL response to determine if the response should trigger a retry.
 * The browser extension can have multiple Sourcegraph URLs and it is not always known which URL will return
 * a repository or if any of the Sourcegraph URLs have a repository. This means in some cases we need to check if we should trigger
 * the retry block by throwing an error.
 *
 * Conditions:
 * 1. There is no response data.
 * 2. Attempting to fetch a repository returned null. response.data.repository will be undefined if the GraphQL query did not request a repository.
 * 3. resolveRev return null for a commit and the repository was also not currently cloning.
 */
function shouldResponseTriggerRetryOrError(response: any): boolean {
    if (!response || !response.data) {
        return true
    }
    const { repository } = response.data
    if (repository === undefined) {
        return false
    }
    if (repository === null) {
        return true
    }
    if (
        repository.commit === null &&
        (!response.data.repository.mirrorInfo || !response.data.repository.mirrorInfo.cloneInProgress)
    ) {
        return true
    }
    return false
}

/**
 * Does a GraphQL query to the Sourcegraph GraphQL API running under `/.api/graphql`
 */
export const queryGraphQL = (args: GraphQLRequestArgs) => requestGraphQL<QueryResult<IQuery>>(args)

/**
 * Does a GraphQL mutation to the Sourcegraph GraphQL API running under `/.api/graphql`
 */
export const mutateGraphQL = (args: GraphQLRequestArgs) => requestGraphQL<MutationResult>(args)
