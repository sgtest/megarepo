import { boolean } from '@storybook/addon-knobs'
import { Meta, DecoratorFn, Story } from '@storybook/react'
import classNames from 'classnames'
import { subDays } from 'date-fns'

import { isChromatic } from '@sourcegraph/storybook'

import { WebStory } from '../../../components/WebStory'
import { ListBatchChange } from '../../../graphql-operations'

import { BatchChangeNode } from './BatchChangeNode'
import { nodes, now } from './testData'

import styles from './BatchChangeListPage.module.scss'

const decorator: DecoratorFn = story => (
    <div className={classNames(styles.grid, styles.narrow, 'p-3 container')}>{story()}</div>
)

const config: Meta = {
    title: 'web/batches/list/BatchChangeNode',
    decorators: [decorator],
}

export default config

const Template: Story<{ node: ListBatchChange }> = ({ node }) => (
    <WebStory>
        {props => (
            <BatchChangeNode
                {...props}
                node={node}
                displayNamespace={boolean('Display namespace', true)}
                now={isChromatic() ? () => subDays(now, 5) : undefined}
                isExecutionEnabled={false}
            />
        )}
    </WebStory>
)
export const OpenBatchChange = Template.bind({})
OpenBatchChange.args = { node: nodes['Open batch change'] }
OpenBatchChange.storyName = 'Open batch change'

export const FailedDraft = Template.bind({})
FailedDraft.args = { node: nodes['Failed draft'] }
FailedDraft.storyName = 'Failed draft'

export const NoDescription = Template.bind({})
NoDescription.args = { node: nodes['No description'] }
NoDescription.storyName = 'No description'

export const ClosedBatchChange = Template.bind({})
ClosedBatchChange.args = { node: nodes['Closed batch change'] }
ClosedBatchChange.storyName = 'Closed batch change'
