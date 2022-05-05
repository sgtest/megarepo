import React from 'react'

import { gql } from '@apollo/client'
import { createMockClient } from '@apollo/client/testing'
import { Meta } from '@storybook/react'

import { TemporarySettingsContext } from '@sourcegraph/shared/src/settings/temporary/TemporarySettingsProvider'
import {
    InMemoryMockSettingsBackend,
    TemporarySettingsStorage,
} from '@sourcegraph/shared/src/settings/temporary/TemporarySettingsStorage'

import { WebStory } from '../../../components/WebStory'
import { CodeInsightsBackendContext, CodeInsightsGqlBackend } from '../core'
import { DashboardPermissions } from '../pages/dashboards/dashboard-page/utils/get-dashboard-permissions'

import { GaConfirmationModal } from './GaConfirmationModal'

const settingsClient = createMockClient(
    { contents: JSON.stringify({}) },
    gql`
        query {
            temporarySettings {
                contents
            }
        }
    `
)

class CodeInsightExampleBackend extends CodeInsightsGqlBackend {
    public getUiFeatures = () => {
        const permissions: DashboardPermissions = { isConfigurable: true }
        return { licensed: false, permissions }
    }
}
const api = new CodeInsightExampleBackend({} as any)

const Story: Meta = {
    title: 'web/insights/GaConfirmationModal',
    decorators: [story => <WebStory>{() => <div className="p-3 container web-content">{story()}</div>}</WebStory>],
}

export default Story

export const GaConfirmationModalExample: React.FunctionComponent<React.PropsWithChildren<unknown>> = () => {
    const settingsStorage = new TemporarySettingsStorage(settingsClient, true)

    settingsStorage.setSettingsBackend(new InMemoryMockSettingsBackend({}))

    return (
        <CodeInsightsBackendContext.Provider value={api}>
            <TemporarySettingsContext.Provider value={settingsStorage}>
                <div>
                    <h2>Some content</h2>
                    <GaConfirmationModal />
                </div>
            </TemporarySettingsContext.Provider>
        </CodeInsightsBackendContext.Provider>
    )
}
