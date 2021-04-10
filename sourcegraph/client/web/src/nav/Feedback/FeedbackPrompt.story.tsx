import { storiesOf } from '@storybook/react'
import { createBrowserHistory } from 'history'
import React from 'react'

import { WebStory } from '../../components/WebStory'

import { FeedbackPrompt } from './FeedbackPrompt'

const history = createBrowserHistory()

const { add } = storiesOf('web/nav', module)

add(
    'Feedback Widget',
    () => <WebStory>{() => <FeedbackPrompt open={true} history={history} routes={[]} />}</WebStory>,
    {
        design: {
            type: 'figma',
            url:
                'https://www.figma.com/file/9FprSCL6roIZotcWMvJZuE/Improving-user-feedback-channels?node-id=300%3A3555',
        },
    }
)
