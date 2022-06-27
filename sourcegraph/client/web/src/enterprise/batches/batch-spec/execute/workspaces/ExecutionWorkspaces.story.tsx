import { DecoratorFn, Meta, Story } from '@storybook/react'
import { of } from 'rxjs'
import { MATCH_ANY_PARAMETERS, WildcardMockLink } from 'wildcard-mock-link'

import { getDocumentNode } from '@sourcegraph/http-client'
import { BatchSpecSource } from '@sourcegraph/shared/src/schema'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import { WebStory } from '../../../../../components/WebStory'
import { BatchSpecWorkspaceResolutionState } from '../../../../../graphql-operations'
import { GET_BATCH_CHANGE_TO_EDIT, WORKSPACE_RESOLUTION_STATUS } from '../../../create/backend'
import {
    mockBatchChange,
    mockFullBatchSpec,
    mockWorkspace,
    mockWorkspaceResolutionStatus,
    mockWorkspaces,
} from '../../batch-spec.mock'
import { BatchSpecContextProvider } from '../../BatchSpecContext'
import { BATCH_SPEC_WORKSPACES, BATCH_SPEC_WORKSPACE_BY_ID, FETCH_BATCH_SPEC_EXECUTION } from '../backend'

import { ExecutionWorkspaces } from './ExecutionWorkspaces'

const decorator: DecoratorFn = story => (
    <div className="p-3 d-flex" style={{ height: '95vh', width: '100%' }}>
        {story()}
    </div>
)

const config: Meta = {
    title: 'web/batches/batch-spec/execute/ExecutionWorkspaces',
    decorators: [decorator],
}

export default config

const MOCKS = new WildcardMockLink([
    {
        request: {
            query: getDocumentNode(GET_BATCH_CHANGE_TO_EDIT),
            variables: MATCH_ANY_PARAMETERS,
        },
        result: { data: { batchChange: mockBatchChange() } },
        nMatches: Number.POSITIVE_INFINITY,
    },
    {
        request: {
            query: getDocumentNode(FETCH_BATCH_SPEC_EXECUTION),
            variables: MATCH_ANY_PARAMETERS,
        },
        result: { data: { node: mockFullBatchSpec() } },
        nMatches: Number.POSITIVE_INFINITY,
    },
    {
        request: {
            query: getDocumentNode(BATCH_SPEC_WORKSPACES),
            variables: MATCH_ANY_PARAMETERS,
        },
        result: { data: mockWorkspaces(50) },
        nMatches: Number.POSITIVE_INFINITY,
    },
    {
        request: {
            query: getDocumentNode(WORKSPACE_RESOLUTION_STATUS),
            variables: MATCH_ANY_PARAMETERS,
        },
        result: { data: mockWorkspaceResolutionStatus(BatchSpecWorkspaceResolutionState.COMPLETED) },
        nMatches: Number.POSITIVE_INFINITY,
    },
    {
        request: {
            query: getDocumentNode(BATCH_SPEC_WORKSPACE_BY_ID),
            variables: MATCH_ANY_PARAMETERS,
        },
        result: { data: { node: mockWorkspace() } },
        nMatches: Number.POSITIVE_INFINITY,
    },
])

const queryEmptyFileDiffs = () => of({ totalCount: 0, pageInfo: { endCursor: null, hasNextPage: false }, nodes: [] })

export const List: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={MOCKS}>
                <BatchSpecContextProvider batchChange={mockBatchChange()} batchSpec={mockFullBatchSpec()}>
                    <ExecutionWorkspaces {...props} />
                </BatchSpecContextProvider>
            </MockedTestProvider>
        )}
    </WebStory>
)

export const WorkspaceSelected: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={MOCKS}>
                <BatchSpecContextProvider batchChange={mockBatchChange()} batchSpec={mockFullBatchSpec()}>
                    <ExecutionWorkspaces
                        {...props}
                        selectedWorkspaceID="spec1234"
                        queryChangesetSpecFileDiffs={queryEmptyFileDiffs}
                    />
                </BatchSpecContextProvider>
            </MockedTestProvider>
        )}
    </WebStory>
)

WorkspaceSelected.storyName = 'with workspace selected'

export const LocallyExecutedSpec: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={MOCKS}>
                <BatchSpecContextProvider
                    batchChange={mockBatchChange()}
                    batchSpec={mockFullBatchSpec({ source: BatchSpecSource.LOCAL })}
                >
                    <div className="container">
                        <ExecutionWorkspaces
                            {...props}
                            selectedWorkspaceID="spec1234"
                            queryChangesetSpecFileDiffs={queryEmptyFileDiffs}
                        />
                    </div>
                </BatchSpecContextProvider>
            </MockedTestProvider>
        )}
    </WebStory>
)

LocallyExecutedSpec.storyName = 'for a locally-executed spec'
