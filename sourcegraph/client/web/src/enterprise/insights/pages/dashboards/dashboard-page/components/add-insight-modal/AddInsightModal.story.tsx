import { useState } from 'react'

import { DecoratorFn, Story, Meta } from '@storybook/react'
import { of } from 'rxjs'

import { WebStory } from '../../../../../../../components/WebStory'
import { CodeInsightsBackendStoryMock } from '../../../../../CodeInsightsBackendStoryMock'
import { AccessibleInsightInfo } from '../../../../../core/backend/code-insights-backend-types'
import { CodeInsightsGqlBackend } from '../../../../../core/backend/gql-backend/code-insights-gql-backend'
import { CustomInsightDashboard, InsightsDashboardOwnerType, InsightsDashboardType } from '../../../../../core/types'

import { AddInsightModal } from './AddInsightModal'

const decorator: DecoratorFn = story => <WebStory>{() => story()}</WebStory>

const config: Meta = {
    title: 'web/insights/AddInsightModal',
    decorators: [decorator],
    parameters: {
        chromatic: {
            viewports: [576, 1440],
        },
    },
}

export default config

const dashboard: CustomInsightDashboard = {
    type: InsightsDashboardType.Custom,
    id: '001',
    title: 'Test dashboard',
    insightIds: [],
    owners: [{ id: '001', title: 'Hieronymus Bosch', type: InsightsDashboardOwnerType.Personal }],
}

const mockInsights: AccessibleInsightInfo[] = [
    {
        id: 'searchInsights.insight.personalGraphQLTypesMigration',
        title: '[Personal] Migration to new GraphQL TS types',
    },
    {
        id: 'searchInsights.insight.testOrg1graphQLTypesMigration',
        title:
            '[Test ORG 1] Migration to new GraphQL TS types [Test ORG 1] Migration to new GraphQL TS types [Test ORG 1] Migration to new GraphQL TS types',
    },
    {
        id: 'searchInsights.insight.testOrg1graphQLTypesMigration1',
        title: '[Test ORG 1] Migration to new GraphQL TS types',
    },
    {
        id: 'searchInsights.insight.testOrg1graphQLTypesMigration2',
        title: '[Test ORG 1] Migration to new GraphQL TS types',
    },
    {
        id: 'searchInsights.insight.testOrg2graphQLTypesMigration',
        title: '[Test ORG 2] Migration to new GraphQL TS types',
    },
]

const codeInsightsBackend: Partial<CodeInsightsGqlBackend> = {
    getAccessibleInsightsList: () => of(mockInsights),
    getDashboardOwners: () => of([]),
}

export const AddInsightModalStory: Story = () => {
    const [open, setOpen] = useState<boolean>(true)

    return (
        <CodeInsightsBackendStoryMock mocks={codeInsightsBackend}>
            {open && <AddInsightModal dashboard={dashboard} onClose={() => setOpen(false)} />}
        </CodeInsightsBackendStoryMock>
    )
}

AddInsightModalStory.storyName = 'AddInsightModal'
