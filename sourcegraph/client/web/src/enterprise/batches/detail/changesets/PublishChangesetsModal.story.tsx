import { action } from '@storybook/addon-actions'
import { Story, Meta, DecoratorFn } from '@storybook/react'
import { noop } from 'lodash'

import { WebStory } from '../../../../components/WebStory'

import { PublishChangesetsModal } from './PublishChangesetsModal'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/details/PublishChangesetsModal',
    decorators: [decorator],
}

export default config

const publishChangesets = () => {
    action('PublishChangesets')
    return Promise.resolve()
}

export const Confirmation: Story = () => (
    <WebStory>
        {props => (
            <PublishChangesetsModal
                {...props}
                afterCreate={noop}
                batchChangeID="test-123"
                changesetIDs={['test-123', 'test-234']}
                onCancel={noop}
                publishChangesets={publishChangesets}
            />
        )}
    </WebStory>
)
