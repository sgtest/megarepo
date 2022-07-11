import { asError } from '@sourcegraph/common'
import { GRAPHQL_URI, GraphQLResult } from '@sourcegraph/http-client'

import { getAccessToken, getInstanceURL } from '..'

export const requestGraphQL = async <R, V = object>(
    request: string,
    variables: V,
    abortSignal?: AbortSignal
): Promise<GraphQLResult<R>> => {
    const instanceURL = getInstanceURL()
    const accessToken = getAccessToken()

    const nameMatch = request.match(/^\s*(?:query|mutation)\s+(\w+)/)
    const apiURL = `${GRAPHQL_URI}${nameMatch ? '?' + nameMatch[1] : ''}`

    const headers = new Headers()
    headers.set('Content-Type', 'application/json')
    headers.set('X-Sourcegraph-Should-Trace', new URLSearchParams(window.location.search).get('trace') || 'false')
    if (accessToken) {
        headers.set('Authorization', `token ${accessToken}`)
    }

    let response: Response | null = null
    try {
        const url = new URL(apiURL, instanceURL).href
        response = await fetch(url, {
            body: JSON.stringify({
                query: request,
                variables,
            }),
            method: 'POST',
            headers,
            signal: abortSignal,
        })
    } catch (error) {
        console.log('Error requesting GraphQL', error, response)
        throw asError(error)
    }

    if (!response || !response.ok) {
        throw new Error(`GraphQL request failed: ${response.status} ${response.statusText}`)
    }

    return (await response.json()) as GraphQLResult<R>
}
