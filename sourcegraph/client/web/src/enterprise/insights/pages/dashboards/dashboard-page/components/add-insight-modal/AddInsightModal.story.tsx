import { storiesOf } from '@storybook/react'
import React, { useState } from 'react'

import { Settings } from '@sourcegraph/shared/src/schema/settings.schema'
import { ConfiguredSubjectOrError, SettingsCascadeOrError } from '@sourcegraph/shared/src/settings/settings'

import { WebStory } from '../../../../../../../components/WebStory'
import { CodeInsightsBackendContext } from '../../../../../core/backend/code-insights-backend-context'
import { CodeInsightsSettingsCascadeBackend } from '../../../../../core/backend/setting-based-api/code-insights-setting-cascade-backend'
import { InsightsDashboardType, InsightsDashboardScope, CustomInsightDashboard } from '../../../../../core/types'

import { AddInsightModal } from './AddInsightModal'

const { add } = storiesOf('web/insights/AddInsightModal', module)
    .addDecorator(story => <WebStory>{() => story()}</WebStory>)
    .addParameters({
        chromatic: {
            viewports: [576, 1440],
        },
    })

const dashboard: CustomInsightDashboard = {
    type: InsightsDashboardType.Custom,
    scope: InsightsDashboardScope.Personal,
    id: '001',
    settingsKey: 'testDashboard',
    title: 'Test dashboard',
    insightIds: [],
    owner: {
        id: 'user_test_id',
        name: 'Emir Kusturica',
    },
}

const ORG_1_SETTINGS: ConfiguredSubjectOrError = {
    lastID: 100,
    settings: {
        'searchInsights.insight.testOrg1graphQLTypesMigration': {
            title:
                '[Test ORG 1] Migration to new GraphQL TS types [Test ORG 1] Migration to new GraphQL TS types [Test ORG 1] Migration to new GraphQL TS types',
            repositories: ['github.com/sourcegraph/sourcegraph'],
            series: [],
            step: { weeks: 6 },
        },
        'searchInsights.insight.testOrg1graphQLTypesMigration1': {
            title: '[Test ORG 1] Migration to new GraphQL TS types',
            repositories: ['github.com/sourcegraph/sourcegraph'],
            series: [],
            step: { weeks: 6 },
        },
        'searchInsights.insight.testOrg1graphQLTypesMigration2': {
            title: '[Test ORG 1] Migration to new GraphQL TS types',
            repositories: ['github.com/sourcegraph/sourcegraph'],
            series: [],
            step: { weeks: 6 },
        },
    },
    subject: {
        __typename: 'Org' as const,
        name: 'test organization 1',
        displayName: 'Test organization 1 Test organization 1 Test organization 1',
        viewerCanAdminister: true,
        id: 'test_org_1_id',
    },
}

const ORG_2_SETTINGS: ConfiguredSubjectOrError = {
    lastID: 101,
    settings: {
        'searchInsights.insight.testOrg2graphQLTypesMigration': {
            title: '[Test ORG 2] Migration to new GraphQL TS types',
            repositories: ['github.com/sourcegraph/sourcegraph'],
            series: [],
            step: { weeks: 6 },
        },
    },
    subject: {
        __typename: 'Org' as const,
        name: 'test organization 2',
        displayName: 'Test organization 2',
        viewerCanAdminister: true,
        id: 'test_org_2_id',
    },
}

const USER_SETTINGS: ConfiguredSubjectOrError = {
    lastID: 102,
    settings: {
        'searchInsights.insight.personalGraphQLTypesMigration': {
            title: '[Personal] Migration to new GraphQL TS types',
            repositories: ['github.com/sourcegraph/sourcegraph'],
            series: [],
            step: { weeks: 6 },
        },
    },
    subject: {
        __typename: 'User' as const,
        id: 'user_test_id',
        username: 'testusername',
        displayName: 'test',
        viewerCanAdminister: true,
    },
}

const SETTINGS_CASCADE: SettingsCascadeOrError<Settings> = {
    subjects: [ORG_1_SETTINGS, ORG_2_SETTINGS, USER_SETTINGS],
    final: {
        // Naive merging of subject settings file for testing UI.
        ...ORG_1_SETTINGS.settings,
        ...ORG_2_SETTINGS.settings,
        ...USER_SETTINGS.settings,
    },
}

const codeInsightsBackend = new CodeInsightsSettingsCascadeBackend(SETTINGS_CASCADE, {} as any)
add('AddInsightModal', () => {
    const [open, setOpen] = useState<boolean>(true)

    return (
        <CodeInsightsBackendContext.Provider value={codeInsightsBackend}>
            {open && <AddInsightModal dashboard={dashboard} onClose={() => setOpen(false)} />}
        </CodeInsightsBackendContext.Provider>
    )
})
