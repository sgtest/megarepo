import { Meta, Story } from '@storybook/react'
import delay from 'delay'
import { noop } from 'lodash'
import React from 'react'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { WebStory } from '../../../../../../components/WebStory'
import { CodeInsightsBackendContext } from '../../../../core/backend/code-insights-backend-context'
import { CodeInsightsSettingsCascadeBackend } from '../../../../core/backend/setting-based-api/code-insights-setting-cascade-backend'
import { SupportedInsightSubject } from '../../../../core/types/subjects'
import {
    createGlobalSubject,
    createOrgSubject,
    createUserSubject,
    SETTINGS_CASCADE_MOCK,
} from '../../../../mocks/settings-cascade'

import { getRandomLangStatsMock } from './components/live-preview-chart/live-preview-mock-data'
import { LangStatsInsightCreationPage as LangStatsInsightCreationPageComponent } from './LangStatsInsightCreationPage'

export default {
    title: 'web/insights/creation-ui/LangStatsInsightCreationPage',
    decorators: [story => <WebStory>{() => story()}</WebStory>],
    parameters: {
        chromatic: {
            viewports: [576, 1440],
            disableSnapshot: false,
        },
    },
} as Meta

function sleep(delay: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, delay))
}

const fakeAPIRequest = async () => {
    await delay(1000)

    throw new Error('Network error')
}

class CodeInsightsStoryBackend extends CodeInsightsSettingsCascadeBackend {
    public getLangStatsInsightContent = async () => {
        await sleep(2000)

        return getRandomLangStatsMock()
    }

    public getRepositorySuggestions = async () => {
        await sleep(2000)

        return [
            { id: '1', name: 'github.com/example/sub-repo-1' },
            { id: '2', name: 'github.com/example/sub-repo-2' },
            { id: '3', name: 'github.com/another-example/sub-repo-1' },
            { id: '4', name: 'github.com/another-example/sub-repo-2' },
        ]
    }
}

const codeInsightsBackend = new CodeInsightsStoryBackend(SETTINGS_CASCADE_MOCK, {} as any)

const SUBJECTS = [
    createUserSubject('Emir Kusturica'),
    createOrgSubject('Warner Brothers'),
    createOrgSubject('Jim Jarmusch Org'),
    createGlobalSubject('Global'),
] as SupportedInsightSubject[]

export const LangStatsInsightCreationPage: Story = () => (
    <CodeInsightsBackendContext.Provider value={codeInsightsBackend}>
        <LangStatsInsightCreationPageComponent
            subjects={SUBJECTS}
            visibility="user_test_id"
            telemetryService={NOOP_TELEMETRY_SERVICE}
            onInsightCreateRequest={fakeAPIRequest}
            onSuccessfulCreation={noop}
            onCancel={noop}
        />
    </CodeInsightsBackendContext.Provider>
)
