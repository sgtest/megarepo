import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { dataOrThrowErrors, gql } from '@sourcegraph/http-client'
import * as GQL from '@sourcegraph/shared/src/schema'

import { requestGraphQL } from '../backend/graphql'
import {
    CreateNotebookResult,
    CreateNotebookStarResult,
    CreateNotebookStarVariables,
    CreateNotebookVariables,
    DeleteNotebookResult,
    DeleteNotebookStarResult,
    DeleteNotebookStarVariables,
    DeleteNotebookVariables,
    FetchNotebookResult,
    FetchNotebookVariables,
    ListNotebooksResult,
    ListNotebooksVariables,
    Maybe,
    NotebookFields,
    Scalars,
    UpdateNotebookResult,
    UpdateNotebookVariables,
} from '../graphql-operations'

const notebooksFragment = gql`
    fragment NotebookFields on Notebook {
        __typename
        id
        title
        creator {
            username
        }
        updater {
            username
        }
        namespace {
            __typename
            id
            namespaceName
        }
        createdAt
        updatedAt
        public
        viewerCanManage
        viewerHasStarred
        stars {
            totalCount
        }
        blocks {
            ... on MarkdownBlock {
                __typename
                id
                markdownInput
            }
            ... on QueryBlock {
                __typename
                id
                queryInput
            }
            ... on FileBlock {
                __typename
                id
                fileInput {
                    __typename
                    repositoryName
                    filePath
                    revision
                    lineRange {
                        __typename
                        startLine
                        endLine
                    }
                }
            }
            ... on SymbolBlock {
                __typename
                id
                symbolInput {
                    __typename
                    repositoryName
                    filePath
                    revision
                    lineContext
                    symbolName
                    symbolContainerName
                    symbolKind
                }
            }
            ... on ComputeBlock {
                __typename
                id
                computeInput
            }
        }
    }
`

const fetchNotebooksQuery = gql`
    query ListNotebooks(
        $first: Int!
        $after: String
        $orderBy: NotebooksOrderBy
        $descending: Boolean
        $creatorUserID: ID
        $starredByUserID: ID
        $namespace: ID
        $query: String
    ) {
        notebooks(
            first: $first
            after: $after
            orderBy: $orderBy
            descending: $descending
            creatorUserID: $creatorUserID
            starredByUserID: $starredByUserID
            namespace: $namespace
            query: $query
        ) {
            nodes {
                ...NotebookFields
            }
            totalCount
            pageInfo {
                endCursor
                hasNextPage
            }
        }
    }
    ${notebooksFragment}
`

export function fetchNotebooks({
    first,
    creatorUserID,
    starredByUserID,
    namespace,
    query,
    after,
    orderBy,
    descending,
}: {
    first: number
    query?: string
    creatorUserID?: Maybe<Scalars['ID']>
    starredByUserID?: Maybe<Scalars['ID']>
    namespace?: Maybe<Scalars['ID']>
    after?: string
    orderBy?: GQL.NotebooksOrderBy
    descending?: boolean
}): Observable<ListNotebooksResult['notebooks']> {
    return requestGraphQL<ListNotebooksResult, ListNotebooksVariables>(fetchNotebooksQuery, {
        first,
        after: after ?? null,
        query: query ?? null,
        creatorUserID: creatorUserID ?? null,
        starredByUserID: starredByUserID ?? null,
        namespace: namespace ?? null,
        orderBy: orderBy ?? GQL.NotebooksOrderBy.NOTEBOOK_UPDATED_AT,
        descending: descending ?? true,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => data.notebooks)
    )
}

const fetchNotebookQuery = gql`
    query FetchNotebook($id: ID!) {
        node(id: $id) {
            ... on Notebook {
                ...NotebookFields
            }
        }
    }
    ${notebooksFragment}
`

export function fetchNotebook(id: Scalars['ID']): Observable<NotebookFields> {
    return requestGraphQL<FetchNotebookResult, FetchNotebookVariables>(fetchNotebookQuery, { id }).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (data.node?.__typename !== 'Notebook') {
                throw new Error('Not a valid notebook')
            }
            return data.node
        })
    )
}

const createNotebookMutation = gql`
    mutation CreateNotebook($notebook: NotebookInput!) {
        createNotebook(notebook: $notebook) {
            ...NotebookFields
        }
    }
    ${notebooksFragment}
`

export function createNotebook(variables: CreateNotebookVariables): Observable<NotebookFields> {
    return requestGraphQL<CreateNotebookResult, CreateNotebookVariables>(createNotebookMutation, variables).pipe(
        map(dataOrThrowErrors),
        map(data => data.createNotebook)
    )
}

const updateNotebookMutation = gql`
    mutation UpdateNotebook($id: ID!, $notebook: NotebookInput!) {
        updateNotebook(id: $id, notebook: $notebook) {
            ...NotebookFields
        }
    }
    ${notebooksFragment}
`

export function updateNotebook(variables: UpdateNotebookVariables): Observable<NotebookFields> {
    return requestGraphQL<UpdateNotebookResult, UpdateNotebookVariables>(updateNotebookMutation, variables).pipe(
        map(dataOrThrowErrors),
        map(data => data.updateNotebook)
    )
}

const deleteNotebookMutation = gql`
    mutation DeleteNotebook($id: ID!) {
        deleteNotebook(id: $id) {
            alwaysNil
        }
    }
`

export function deleteNotebook(id: GQL.ID): Observable<DeleteNotebookResult> {
    return requestGraphQL<DeleteNotebookResult, DeleteNotebookVariables>(deleteNotebookMutation, { id }).pipe(
        map(dataOrThrowErrors)
    )
}

const createNotebookStarMutation = gql`
    mutation CreateNotebookStar($notebookID: ID!) {
        createNotebookStar(notebookID: $notebookID) {
            createdAt
        }
    }
`

export function createNotebookStar(notebookID: GQL.ID): Observable<CreateNotebookStarResult['createNotebookStar']> {
    return requestGraphQL<CreateNotebookStarResult, CreateNotebookStarVariables>(createNotebookStarMutation, {
        notebookID,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => data.createNotebookStar)
    )
}

const deleteNotebookStarMutation = gql`
    mutation DeleteNotebookStar($notebookID: ID!) {
        deleteNotebookStar(notebookID: $notebookID) {
            alwaysNil
        }
    }
`

export function deleteNotebookStar(notebookID: GQL.ID): Observable<DeleteNotebookStarResult> {
    return requestGraphQL<DeleteNotebookStarResult, DeleteNotebookStarVariables>(deleteNotebookStarMutation, {
        notebookID,
    }).pipe(map(dataOrThrowErrors))
}
