import * as React from 'react'
import { createMemoryHistory } from 'history'
import { storiesOf } from '@storybook/react'
import { WebStory } from '../../../../components/WebStory'
import { Progress } from '../../../stream'
import { StreamingProgressSkippedPopover } from './StreamingProgressSkippedPopover'

const history = createMemoryHistory()
const { add } = storiesOf(
    'web/search/results/streaming/progress/StreamingProgressSkippedPopover',
    module
).addParameters({
    design: {
        type: 'figma',
        url: 'https://www.figma.com/file/IyiXZIbPHK447NCXov0AvK/13928-Streaming-search?node-id=280%3A17768',
    },
    chromatic: { viewports: [350] },
})

add('popover', () => {
    const progress: Progress = {
        durationMs: 1500,
        matchCount: 2,
        repositoriesCount: 2,
        skipped: [
            {
                reason: 'excluded-fork',
                message: '',
                severity: 'info',
                title: '10k forked repositories excluded',
                suggested: {
                    title: 'include forked',
                    queryExpression: 'fork:yes',
                },
            },
            {
                reason: 'error',
                message:
                    'There was a network error retrieving search results. Check your Internet connection and try again.',
                severity: 'error',
                title: 'Error loading results',
            },
            {
                reason: 'excluded-archive',
                message: '',
                severity: 'info',
                title: '60k archived repositories excluded',
                suggested: {
                    title: 'include archived',
                    queryExpression: 'archived:yes',
                },
            },
            {
                reason: 'excluded-archive',
                message: 'By default we exclude archived repositories. Include them with `archived:yes` in your query.',
                severity: 'info',
                title: '1 archived',
                suggested: {
                    title: 'include archived',
                    queryExpression: 'archived:yes',
                },
            },
            {
                reason: 'shard-timedout',
                message:
                    'Search timed out before some repositories could be searched. Try reducing scope of your query with repo: or other filters.',
                severity: 'warn',
                title: 'Search timed out',
                suggested: {
                    title: 'increase timeout',
                    queryExpression: 'timeout:2m',
                },
            },
        ],
    }

    return (
        <WebStory>
            {() => <StreamingProgressSkippedPopover progress={progress} onSearchAgain={() => {}} history={history} />}
        </WebStory>
    )
})
