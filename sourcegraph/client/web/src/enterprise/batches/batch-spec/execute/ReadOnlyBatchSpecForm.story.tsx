import type { Decorator, Meta, StoryFn } from '@storybook/react'

import { WebStory } from '../../../../components/WebStory'
import { BatchSpecSource, BatchSpecState } from '../../../../graphql-operations'
import { mockBatchChange, mockFullBatchSpec } from '../batch-spec.mock'
import { BatchSpecContextProvider } from '../BatchSpecContext'

import { ReadOnlyBatchSpecForm } from './ReadOnlyBatchSpecForm'

const decorator: Decorator = story => (
    <div className="p-3 d-flex" style={{ height: '95vh', width: '100%' }}>
        {story()}
    </div>
)
const config: Meta = {
    title: 'web/batches/batch-spec/execute/ReadOnlyBatchSpecForm',
    decorators: [decorator],
}

export default config

export const Executing: Story = args => (
    <WebStory>
        {props => (
            <BatchSpecContextProvider
                batchChange={mockBatchChange()}
                batchSpec={mockFullBatchSpec({
                    state: args.state,
                })}
            >
                <ReadOnlyBatchSpecForm {...props} />
            </BatchSpecContextProvider>
        )}
    </WebStory>
)
Executing.argTypes = {
    state: {
        name: 'batch spec state',
        control: { type: 'select', options: [BatchSpecState.PROCESSING, BatchSpecState.QUEUED] },
    },
}
Executing.args = {
    state: BatchSpecState.PROCESSING,
}

Executing.storyName = 'while executing'

export const ExecutionFinished: Story = args => (
    <WebStory>
        {props => (
            <BatchSpecContextProvider
                batchChange={mockBatchChange()}
                batchSpec={mockFullBatchSpec({
                    state: args.state,
                })}
            >
                <ReadOnlyBatchSpecForm {...props} />
            </BatchSpecContextProvider>
        )}
    </WebStory>
)
ExecutionFinished.argTypes = {
    state: {
        name: 'batch spec state',
        control: {
            type: 'select',
            options: [
                BatchSpecState.CANCELED,
                BatchSpecState.CANCELING,
                BatchSpecState.COMPLETED,
                BatchSpecState.FAILED,
                BatchSpecState.PENDING,
            ],
        },
    },
}
ExecutionFinished.args = {
    state: BatchSpecState.COMPLETED,
}

ExecutionFinished.storyName = 'after execution finishes'

export const LocallyExecutedSpec: StoryFn = () => (
    <WebStory>
        {props => (
            <BatchSpecContextProvider
                batchChange={mockBatchChange()}
                batchSpec={mockFullBatchSpec({ source: BatchSpecSource.LOCAL })}
            >
                <ReadOnlyBatchSpecForm {...props} />
            </BatchSpecContextProvider>
        )}
    </WebStory>
)

LocallyExecutedSpec.storyName = 'for a locally-executed spec'
