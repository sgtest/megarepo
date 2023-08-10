import { useState } from 'react'

import type { DecoratorFn, Story, Meta } from '@storybook/react'

import { WebStory } from '../../../components/WebStory'

import { TabBar, type TabsConfig, type TabKey } from './TabBar'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/batch-spec/TabBar',
    decorators: [decorator],
}

export default config

const CREATE_TABS: TabsConfig[] = [{ key: 'configuration', isEnabled: true }]

export const CreateNewBatchChange: Story = () => (
    <WebStory>{props => <TabBar {...props} activeTabKey="configuration" tabsConfig={CREATE_TABS} />}</WebStory>
)

CreateNewBatchChange.storyName = 'creating a new batch change'

export const EditUnexecutedBatchSpec: Story = () => {
    const [activeTabKey, setActiveTabKey] = useState<TabKey>('spec')

    const tabsConfig: TabsConfig[] = [
        {
            key: 'configuration',
            isEnabled: true,
            handler: {
                type: 'button',
                onClick: () => setActiveTabKey('configuration'),
            },
        },
        {
            key: 'spec',
            isEnabled: true,
            handler: {
                type: 'button',
                onClick: () => setActiveTabKey('spec'),
            },
        },
    ]

    return <WebStory>{props => <TabBar {...props} tabsConfig={tabsConfig} activeTabKey={activeTabKey} />}</WebStory>
}

EditUnexecutedBatchSpec.storyName = 'editing unexecuted batch spec'

const EXECUTING_TABS: TabsConfig[] = [
    { key: 'configuration', isEnabled: true, handler: { type: 'link' } },
    { key: 'spec', isEnabled: true, handler: { type: 'link' } },
    { key: 'execution', isEnabled: true, handler: { type: 'link' } },
]

export const ExecuteBatchSpec: Story = args => (
    <WebStory>{props => <TabBar {...props} tabsConfig={EXECUTING_TABS} activeTabKey={args.activeTabKey} />}</WebStory>
)
ExecuteBatchSpec.argTypes = {
    activeTabKey: {
        control: { type: 'select', options: ['configuration', 'spec', 'execution'] },
        defaultValue: 'execution',
    },
}

ExecuteBatchSpec.storyName = 'executing a batch spec'

const PREVIEWING_TABS: TabsConfig[] = [
    { key: 'configuration', isEnabled: true, handler: { type: 'link' } },
    { key: 'spec', isEnabled: true, handler: { type: 'link' } },
    { key: 'execution', isEnabled: true, handler: { type: 'link' } },
    { key: 'preview', isEnabled: true, handler: { type: 'link' } },
]

export const PreviewExecutionResult: Story = args => (
    <WebStory>{props => <TabBar {...props} tabsConfig={PREVIEWING_TABS} activeTabKey={args.activeTabKey} />}</WebStory>
)
PreviewExecutionResult.argTypes = {
    activeTabKey: {
        control: { type: 'select', options: ['configuration', 'spec', 'execution', 'preview'] },
        defaultValue: 'preview',
    },
}

PreviewExecutionResult.storyName = 'previewing an execution result'

const LOCAL_TABS: TabsConfig[] = [
    { key: 'configuration', isEnabled: true, handler: { type: 'link' } },
    { key: 'spec', isEnabled: true, handler: { type: 'link' } },
    { key: 'execution', isEnabled: false },
    { key: 'preview', isEnabled: true, handler: { type: 'link' } },
]

export const LocallyExecutedSpec: Story = args => (
    <WebStory>{props => <TabBar {...props} tabsConfig={LOCAL_TABS} activeTabKey={args.activeTabKey} />}</WebStory>
)
LocallyExecutedSpec.argTypes = {
    activeTabKey: {
        control: { type: 'select', options: ['configuration', 'spec', 'preview'] },
        defaultValue: 'preview',
    },
}

LocallyExecutedSpec.storyName = 'for a locally-executed spec'
