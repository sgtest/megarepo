import { Observable } from 'rxjs'
import { Omit } from 'utility-types'
import { createAggregateError } from '../util/errors'
import { checkOk } from '../backend/fetch'
import * as GQL from './schema'
import { fromFetch } from './fromFetch'

/**
 * Use this template string tag for all GraphQL queries.
 */
export const gql = (template: TemplateStringsArray, ...substitutions: any[]): string =>
    String.raw(template, ...substitutions)

export interface SuccessGraphQLResult<T extends GQL.IQuery | GQL.IMutation> {
    data: T
    errors: undefined
}
export interface ErrorGraphQLResult {
    data: undefined
    errors: GQL.IGraphQLResponseError[]
}

export type GraphQLResult<T extends GQL.IQuery | GQL.IMutation> = SuccessGraphQLResult<T> | ErrorGraphQLResult

/**
 * Guarantees that the GraphQL query resulted in an error.
 */
export function isErrorGraphQLResult<T extends GQL.IQuery | GQL.IMutation>(
    result: GraphQLResult<T>
): result is ErrorGraphQLResult {
    return !!(result as ErrorGraphQLResult).errors && (result as ErrorGraphQLResult).errors.length > 0
}

export function dataOrThrowErrors<T extends GQL.IQuery | GQL.IMutation>(result: GraphQLResult<T>): T {
    if (isErrorGraphQLResult(result)) {
        throw createAggregateError(result.errors)
    }
    return result.data
}

export interface GraphQLError extends Error {
    queryName: string
}
export const createInvalidGraphQLQueryResponseError = (queryName: string): GraphQLError =>
    Object.assign(new Error(`Invalid GraphQL response: query ${queryName}`), {
        queryName,
    })
export const createInvalidGraphQLMutationResponseError = (queryName: string): GraphQLError =>
    Object.assign(new Error(`Invalid GraphQL response: mutation ${queryName}`), {
        queryName,
    })

export interface GraphQLRequestOptions extends Omit<RequestInit, 'method' | 'body'> {
    baseUrl?: string
}

export function requestGraphQL<T extends GQL.IQuery | GQL.IMutation>({
    request,
    variables = {},
    baseUrl = '',
    ...options
}: GraphQLRequestOptions & {
    request: string
    variables?: {}
}): Observable<GraphQLResult<T>> {
    const nameMatch = request.match(/^\s*(?:query|mutation)\s+(\w+)/)
    return fromFetch(
        `${baseUrl}/.api/graphql${nameMatch ? '?' + nameMatch[1] : ''}`,
        {
            ...options,
            method: 'POST',
            body: JSON.stringify({ query: request, variables }),
        },
        response => checkOk(response).json()
    )
}
