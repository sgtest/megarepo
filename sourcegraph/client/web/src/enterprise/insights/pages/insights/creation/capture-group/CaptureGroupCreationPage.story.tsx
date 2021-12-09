import { Meta } from '@storybook/react'
import React from 'react'

import { WebStory } from '../../../../../../components/WebStory'
import { LINE_CHART_WITH_HUGE_NUMBER_OF_LINES } from '../../../../../../views/mocks/charts-content'
import { CodeInsightsBackendContext } from '../../../../core/backend/code-insights-backend-context'
import { CodeInsightsGqlBackend } from '../../../../core/backend/gql-api/code-insights-gql-backend'

import { CaptureGroupCreationPage } from './CaptureGroupCreationPage'

export default {
    title: 'web/insights/creation-ui/CaptureGroupCreationPage',
    decorators: [story => <WebStory>{() => <div className="p-3 container web-content">{story()}</div>}</WebStory>],
} as Meta

class CodeInsightExampleBackend extends CodeInsightsGqlBackend {
    public getRepositorySuggestions = () =>
        Promise.resolve([
            { id: '1', name: 'github.com/example/sub-repo-1' },
            { id: '2', name: 'github.com/example/sub-repo-2' },
            { id: '3', name: 'github.com/another-example/sub-repo-1' },
            { id: '4', name: 'github.com/another-example/sub-repo-2' },
        ])

    public getCaptureInsightContent = () => Promise.resolve(LINE_CHART_WITH_HUGE_NUMBER_OF_LINES)
}

const api = new CodeInsightExampleBackend({} as any)

export const CaptureGroupCreationPageExample = () => (
    <CodeInsightsBackendContext.Provider value={api}>
        <CaptureGroupCreationPage />
    </CodeInsightsBackendContext.Provider>
)
