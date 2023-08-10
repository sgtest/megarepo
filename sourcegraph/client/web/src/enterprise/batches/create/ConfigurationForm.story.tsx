import type { DecoratorFn, Meta, Story } from '@storybook/react'
import { MATCH_ANY_PARAMETERS, WildcardMockLink } from 'wildcard-mock-link'

import { getDocumentNode } from '@sourcegraph/http-client'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import type { AuthenticatedUser } from '../../../auth'
import { WebStory } from '../../../components/WebStory'
import { GET_LICENSE_AND_USAGE_INFO } from '../list/backend'
import { getLicenseAndUsageInfoResult } from '../list/testData'

import { ConfigurationForm } from './ConfigurationForm'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/create/ConfigurationForm',
    decorators: [decorator],
    parameters: {
        chromatic: {
            disableSnapshot: false,
        },
    },
}

export default config

const MOCK_ORGANIZATION = {
    __typename: 'Org',
    name: 'acme-corp',
    displayName: 'ACME Corporation',
    id: 'acme-corp-id',
}

const mockAuthenticatedUser = {
    __typename: 'User',
    username: 'alice',
    displayName: 'alice',
    id: 'b',
    organizations: {
        nodes: [MOCK_ORGANIZATION],
    },
} as AuthenticatedUser

const buildMocks = (isLicensed = true, hasBatchChanges = true) =>
    new WildcardMockLink([
        {
            request: { query: getDocumentNode(GET_LICENSE_AND_USAGE_INFO), variables: MATCH_ANY_PARAMETERS },
            result: { data: getLicenseAndUsageInfoResult(isLicensed, hasBatchChanges) },
            nMatches: Number.POSITIVE_INFINITY,
        },
    ])

export const NewBatchChange: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={buildMocks()}>
                <ConfigurationForm authenticatedUser={mockAuthenticatedUser} />
            </MockedTestProvider>
        )}
    </WebStory>
)

NewBatchChange.storyName = 'New batch change'

export const NewOrgBatchChange: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={buildMocks()}>
                <ConfigurationForm
                    {...props}
                    initialNamespaceID={MOCK_ORGANIZATION.id}
                    authenticatedUser={mockAuthenticatedUser}
                />
            </MockedTestProvider>
        )}
    </WebStory>
)

NewOrgBatchChange.storyName = 'New batch change with new Org'

export const ExistingBatchChange: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={buildMocks()}>
                <ConfigurationForm
                    {...props}
                    authenticatedUser={mockAuthenticatedUser}
                    isReadOnly={true}
                    batchChange={{
                        name: 'My existing batch change',
                        namespace: {
                            __typename: 'Org',
                            namespaceName: 'Sourcegraph',
                            displayName: null,
                            name: 'sourcegraph',
                            url: '/orgs/sourcegraph',
                            id: 'test1234',
                        },
                    }}
                />
            </MockedTestProvider>
        )}
    </WebStory>
)

ExistingBatchChange.storyName = 'Read-only for existing batch change'

export const LicenseAlert: Story = () => (
    <WebStory>
        {props => (
            <MockedTestProvider link={buildMocks(false)}>
                <ConfigurationForm
                    {...props}
                    isReadOnly={true}
                    authenticatedUser={mockAuthenticatedUser}
                    batchChange={{
                        name: 'My existing batch change',
                        namespace: {
                            __typename: 'Org',
                            namespaceName: 'Sourcegraph',
                            displayName: null,
                            name: 'sourcegraph',
                            url: '/orgs/sourcegraph',
                            id: 'test1234',
                        },
                    }}
                />
            </MockedTestProvider>
        )}
    </WebStory>
)

LicenseAlert.storyName = 'License alert'
