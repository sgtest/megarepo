import { action } from '@storybook/addon-actions'
import { DecoratorFn, Meta, Story } from '@storybook/react'

import { WebStory } from '../../../../../components/WebStory'
import { mockPreviewWorkspaces } from '../../batch-spec.mock'

import { WorkspacesPreviewList } from './WorkspacesPreviewList'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/batch-spec/edit/workspaces-preview/WorkspacesPreviewList',
    decorators: [decorator],
}

export default config

export const DefaultStory: Story = args => {
    const count = args.count
    return (
        <WebStory>
            {props => (
                <WorkspacesPreviewList
                    isStale={args.isStale}
                    showCached={false}
                    error={undefined}
                    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
                    // @ts-ignore
                    workspacesConnection={{
                        connection: {
                            totalCount: count,
                            nodes: mockPreviewWorkspaces(count),
                            pageInfo: {
                                hasNextPage: false,
                                endCursor: null,
                            },
                        },
                        hasNextPage: args.hasNextPage,
                        fetchMore: action('Fetch More'),
                    }}
                    {...props}
                />
            )}
        </WebStory>
    )
}
DefaultStory.argTypes = {
    count: {
        name: 'name',
        control: { type: 'number' },
        defaultValue: 1,
    },
    isStale: {
        name: 'Stale',
        control: { type: 'boolean' },
        defaultValue: false,
    },
    hasNextPage: {
        name: 'Has Next Page',
        control: { type: 'boolean' },
        defaultValue: false,
    },
}

DefaultStory.storyName = 'default'

export const ErrorStory: Story = () => (
    <WebStory>
        {props => (
            <WorkspacesPreviewList
                showCached={false}
                error="Failed to load workspaces"
                // eslint-disable-next-line @typescript-eslint/ban-ts-comment
                // @ts-ignore
                workspacesConnection={{
                    connection: {
                        totalCount: 0,
                        nodes: [],
                        pageInfo: {
                            hasNextPage: false,
                            endCursor: null,
                        },
                    },
                }}
                {...props}
            />
        )}
    </WebStory>
)
