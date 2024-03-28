import type { Decorator, Meta, StoryFn } from '@storybook/react'

import { noOpTelemetryRecorder } from '@sourcegraph/shared/src/telemetry'

import { WebStory } from '../../components/WebStory'

import { SurveyPage } from './SurveyPage'
import { submitSurveyMock } from './SurveyPage.mocks'

const decorator: Decorator = story => (
    <WebStory mocks={[submitSurveyMock]}>{() => <div className="container mt-3">{story()}</div>}</WebStory>
)

const config: Meta = {
    title: 'web/SurveyPage',
    decorators: [decorator],
}

export default config

export const Page: StoryFn = () => (
    <SurveyPage authenticatedUser={null} forceScore="10" telemetryRecorder={noOpTelemetryRecorder} />
)
