import { DecoratorFn, Meta, Story } from '@storybook/react'

import { MockedStoryProvider } from '@sourcegraph/storybook'

import { WebStory } from '../../components/WebStory'

import { SurveyPage } from './SurveyPage'
import { submitSurveyMock } from './SurveyPage.mocks'

const decorator: DecoratorFn = story => <WebStory>{() => <div className="container mt-3">{story()}</div>}</WebStory>

const config: Meta = {
    title: 'web/SurveyPage',
    decorators: [decorator],
}

export default config

export const Page: Story = () => (
    <MockedStoryProvider mocks={[submitSurveyMock]}>
        <SurveyPage authenticatedUser={null} forceScore="10" />
    </MockedStoryProvider>
)
