import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { dataOrThrowErrors, gql } from '@sourcegraph/shared/src/graphql/graphql'

import { fileDiffFields } from '../../../backend/diff'
import { requestGraphQL } from '../../../backend/graphql'
import {
    BatchSpecExecutionByIDResult,
    BatchSpecExecutionByIDVariables,
    BatchSpecExecutionFields,
    BatchSpecWorkspaceByIDResult,
    BatchSpecWorkspaceByIDVariables,
    BatchSpecWorkspaceFields,
    BatchSpecWorkspaceStepFileDiffsResult,
    BatchSpecWorkspaceStepFileDiffsVariables,
    CancelBatchSpecExecutionResult,
    CancelBatchSpecExecutionVariables,
    Scalars,
    WorkspaceStepFileDiffConnectionFields,
} from '../../../graphql-operations'

const batchSpecWorkspaceFieldsFragment = gql`
    fragment BatchSpecWorkspaceFields on BatchSpecWorkspace {
        id
        steps {
            ...BatchSpecWorkspaceStepFields
        }
        searchResultPaths
        queuedAt
        startedAt
        finishedAt
        failureMessage
        state
        changesetSpecs {
            id
            type
            __typename
            ... on VisibleChangesetSpec {
                description {
                    ... on GitBranchChangesetDescription {
                        title
                    }
                    ... on ExistingChangesetReference {
                        externalID
                    }
                }
            }
        }
        diffStat {
            added
            changed
            deleted
        }
        placeInQueue
        repository {
            name
            url
        }
        branch {
            name
        }
        path
        onlyFetchWorkspace
        ignored
        unsupported
        cachedResultFound
        stages {
            setup {
                ...BatchSpecWorkspaceExecutionLogEntryFields
            }
            srcExec {
                ...BatchSpecWorkspaceExecutionLogEntryFields
            }
            teardown {
                ...BatchSpecWorkspaceExecutionLogEntryFields
            }
        }
    }

    fragment BatchSpecWorkspaceStepFields on BatchSpecWorkspaceStep {
        run
        diffStat {
            added
            changed
            deleted
        }
        container
        cachedResultFound
        skipped
        outputLines
        startedAt
        finishedAt
        exitCode
        environment {
            name
            value
        }
        outputVariables {
            name
            value
        }
    }

    fragment BatchSpecWorkspaceExecutionLogEntryFields on ExecutionLogEntry {
        key
        command
        startTime
        exitCode
        out
        durationMilliseconds
    }
`

const batchSpecExecutionFieldsFragment = gql`
    fragment BatchSpecExecutionFields on BatchSpec {
        id
        originalInput
        state
        createdAt
        startedAt
        finishedAt
        failureMessage
        applyURL
        creator {
            id
            url
            displayName
        }
        namespace {
            id
            url
            namespaceName
        }
        workspaceResolution {
            workspaces(first: 10000) {
                totalCount
                pageInfo {
                    endCursor
                    hasNextPage
                }
                nodes {
                    ...BatchSpecWorkspaceListFields
                }
            }
        }
    }

    fragment BatchSpecWorkspaceListFields on BatchSpecWorkspace {
        id
        state
        diffStat {
            added
            changed
            deleted
        }
        placeInQueue
        repository {
            name
            url
        }
        branch {
            name
        }
        path
        ignored
        unsupported
        cachedResultFound
    }
`

export const fetchBatchSpecExecution = (id: Scalars['ID']): Observable<BatchSpecExecutionFields | null> =>
    requestGraphQL<BatchSpecExecutionByIDResult, BatchSpecExecutionByIDVariables>(
        gql`
            query BatchSpecExecutionByID($id: ID!) {
                node(id: $id) {
                    __typename
                    ...BatchSpecExecutionFields
                }
            }
            ${batchSpecExecutionFieldsFragment}
        `,
        { id }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                return null
            }
            if (node.__typename !== 'BatchSpec') {
                throw new Error(`Node is a ${node.__typename}, not a BatchSpec`)
            }
            return node
        })
    )

export const fetchBatchSpecWorkspace = (id: Scalars['ID']): Observable<BatchSpecWorkspaceFields | null> =>
    requestGraphQL<BatchSpecWorkspaceByIDResult, BatchSpecWorkspaceByIDVariables>(
        gql`
            query BatchSpecWorkspaceByID($id: ID!) {
                node(id: $id) {
                    __typename
                    ...BatchSpecWorkspaceFields
                }
            }
            ${batchSpecWorkspaceFieldsFragment}
        `,
        { id }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                return null
            }
            if (node.__typename !== 'BatchSpecWorkspace') {
                throw new Error(`Node is a ${node.__typename}, not a BatchSpecWorkspace`)
            }
            return node
        })
    )

export async function cancelBatchSpecExecution(id: Scalars['ID']): Promise<BatchSpecExecutionFields> {
    const result = await requestGraphQL<CancelBatchSpecExecutionResult, CancelBatchSpecExecutionVariables>(
        gql`
            mutation CancelBatchSpecExecution($id: ID!) {
                cancelBatchSpecExecution(batchSpec: $id) {
                    ...BatchSpecExecutionFields
                }
            }

            ${batchSpecExecutionFieldsFragment}
        `,
        { id }
    ).toPromise()
    return dataOrThrowErrors(result).cancelBatchSpecExecution
}

const batchSpecWorkspaceStepFileDiffsFields = gql`
    fragment BatchSpecWorkspaceStepFileDiffsFields on BatchSpecWorkspace {
        step(index: $step) {
            diff {
                fileDiffs(first: $first, after: $after) {
                    ...WorkspaceStepFileDiffConnectionFields
                }
            }
        }
    }

    fragment WorkspaceStepFileDiffConnectionFields on FileDiffConnection {
        nodes {
            ...FileDiffFields
        }
        totalCount
        pageInfo {
            hasNextPage
            endCursor
        }
    }

    ${fileDiffFields}
`

export const queryBatchSpecWorkspaceStepFileDiffs = ({
    node: nodeID,
    step,
    first,
    after,
}: BatchSpecWorkspaceStepFileDiffsVariables): Observable<WorkspaceStepFileDiffConnectionFields> =>
    requestGraphQL<BatchSpecWorkspaceStepFileDiffsResult, BatchSpecWorkspaceStepFileDiffsVariables>(
        gql`
            query BatchSpecWorkspaceStepFileDiffs($node: ID!, $step: Int!, $first: Int, $after: String) {
                node(id: $node) {
                    __typename
                    ...BatchSpecWorkspaceStepFileDiffsFields
                }
            }

            ${batchSpecWorkspaceStepFileDiffsFields}
        `,
        { node: nodeID, step, first, after }
    ).pipe(
        map(dataOrThrowErrors),
        map(({ node }) => {
            if (!node) {
                throw new Error(`BatchSpecWorkspace with ID ${nodeID} does not exist`)
            }
            if (node.__typename !== 'BatchSpecWorkspace') {
                throw new Error(`The given ID is a ${node.__typename}, not a BatchSpecWorkspace`)
            }
            if (!node.step) {
                throw new Error('The given Step is not available')
            }
            if (!node.step.diff) {
                throw new Error('The diff is not available')
            }
            return node.step.diff.fileDiffs
        })
    )
