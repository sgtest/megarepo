import { action } from '@storybook/addon-actions'
import { DecoratorFn, Meta, Story } from '@storybook/react'

import { WebStory } from '../../../../../components/WebStory'

import { ReplaceSpecModal } from './ReplaceSpecModal'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/batch-spec/edit/library/ReplaceSpecModal',
    decorators: [decorator],
    argTypes: {
        libraryItemName: {
            name: 'Name',
            control: { type: 'text' },
            defaultValue: 'my-batch-change',
        },
    },
}

export default config

export const ReplaceSpecModalStory: Story = args => (
    <WebStory>
        {props => (
            <ReplaceSpecModal
                libraryItemName={args.libraryItemName}
                onCancel={action('On Cancel')}
                onConfirm={action('On Confirm')}
                {...props}
            />
        )}
    </WebStory>
)

ReplaceSpecModalStory.storyName = 'ReplaceSpecModal'
