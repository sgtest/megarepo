import { storiesOf } from '@storybook/react'
import React from 'react'

import { WebStory } from '../../components/WebStory'

import { FeedbackPrompt } from './FeedbackPrompt'

const { add } = storiesOf('web/nav', module)

add('Feedback Widget', () => <WebStory>{() => <FeedbackPrompt open={true} routes={[]} />}</WebStory>, {
    design: {
        type: 'figma',
        url: 'https://www.figma.com/file/9FprSCL6roIZotcWMvJZuE/Improving-user-feedback-channels?node-id=300%3A3555',
    },
})
