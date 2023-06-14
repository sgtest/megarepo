import { DecoratorFn, Meta, Story } from '@storybook/react'
import { MATCH_ANY_PARAMETERS, WildcardMockedResponse, WildcardMockLink } from 'wildcard-mock-link'

import { getDocumentNode } from '@sourcegraph/http-client'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import { WebStory } from '../../../components/WebStory'
import { BatchChangesCodeHostFields, ExternalServiceKind } from '../../../graphql-operations'
import { BATCH_CHANGES_SITE_CONFIGURATION } from '../backend'
import { rolloutWindowConfigMockResult } from '../mocks'

import { GLOBAL_CODE_HOSTS } from './backend'
import { BatchChangesSiteConfigSettingsPage } from './BatchChangesSiteConfigSettingsPage'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/settings/BatchChangesSiteConfigSettingsPage',
    decorators: [decorator],
}

export default config

const ROLLOUT_WINDOWS_CONFIGURATION_MOCK = {
    request: {
        query: getDocumentNode(BATCH_CHANGES_SITE_CONFIGURATION),
    },
    result: rolloutWindowConfigMockResult,
    nMatches: Number.POSITIVE_INFINITY,
}

const createMock = (...hosts: BatchChangesCodeHostFields[]): WildcardMockedResponse => ({
    request: {
        query: getDocumentNode(GLOBAL_CODE_HOSTS),
        variables: MATCH_ANY_PARAMETERS,
    },
    result: {
        data: {
            batchChangesCodeHosts: {
                totalCount: hosts.length,
                pageInfo: { endCursor: null, hasNextPage: false },
                nodes: hosts,
            },
        },
    },
    nMatches: Number.POSITIVE_INFINITY,
})

export const Overview: Story = () => (
    <WebStory>
        {() => (
            <MockedTestProvider
                link={
                    new WildcardMockLink([
                        ROLLOUT_WINDOWS_CONFIGURATION_MOCK,
                        createMock(
                            {
                                credential: null,
                                externalServiceKind: ExternalServiceKind.GITHUB,
                                externalServiceURL: 'https://github.com/',
                                requiresSSH: false,
                                requiresUsername: false,
                                supportsCommitSigning: true,
                                commitSigningConfiguration: {
                                    __typename: 'GitHubApp',
                                    id: '123',
                                    appID: 123,
                                    name: 'Sourcegraph Commit Signing',
                                    appURL: 'https://github.com/apps/sourcegraph-commit-signing',
                                    baseURL: 'https://github.com/',
                                    logo: 'https://github.com/identicons/app/app/commit-testing-local',
                                },
                            },
                            {
                                credential: null,
                                externalServiceKind: ExternalServiceKind.GITHUB,
                                externalServiceURL: 'https://github.mycompany.com/',
                                requiresSSH: false,
                                requiresUsername: false,
                                supportsCommitSigning: true,
                                commitSigningConfiguration: null,
                            },
                            {
                                credential: null,
                                externalServiceKind: ExternalServiceKind.GITLAB,
                                externalServiceURL: 'https://gitlab.com/',
                                requiresSSH: false,
                                requiresUsername: false,
                                supportsCommitSigning: false,
                                commitSigningConfiguration: null,
                            },
                            {
                                credential: null,
                                externalServiceKind: ExternalServiceKind.BITBUCKETSERVER,
                                externalServiceURL: 'https://bitbucket.sgdev.org/',
                                requiresSSH: true,
                                requiresUsername: false,
                                supportsCommitSigning: false,
                                commitSigningConfiguration: null,
                            },
                            {
                                credential: null,
                                externalServiceKind: ExternalServiceKind.BITBUCKETCLOUD,
                                externalServiceURL: 'https://bitbucket.org/',
                                requiresSSH: false,
                                requiresUsername: true,
                                supportsCommitSigning: false,
                                commitSigningConfiguration: null,
                            }
                        ),
                    ])
                }
            >
                <BatchChangesSiteConfigSettingsPage />
            </MockedTestProvider>
        )}
    </WebStory>
)

export const ConfigAdded: Story = () => (
    <WebStory>
        {() => (
            <MockedTestProvider
                link={
                    new WildcardMockLink([
                        ROLLOUT_WINDOWS_CONFIGURATION_MOCK,
                        createMock(
                            {
                                credential: {
                                    id: '123',
                                    isSiteCredential: true,
                                    sshPublicKey:
                                        'rsa-ssh randorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorando',
                                },
                                externalServiceKind: ExternalServiceKind.GITHUB,
                                externalServiceURL: 'https://github.com/',
                                requiresSSH: false,
                                requiresUsername: false,
                                supportsCommitSigning: true,
                                commitSigningConfiguration: {
                                    __typename: 'GitHubApp',
                                    id: '123',
                                    appID: 123,
                                    name: 'Sourcegraph Commit Signing',
                                    appURL: 'https://github.com/apps/sourcegraph-commit-signing',
                                    baseURL: 'https://github.com/',
                                    logo: 'https://github.com/identicons/app/app/commit-testing-local',
                                },
                            },
                            {
                                credential: {
                                    id: '123',
                                    isSiteCredential: true,
                                    sshPublicKey:
                                        'rsa-ssh randorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorando',
                                },
                                externalServiceKind: ExternalServiceKind.GITLAB,
                                externalServiceURL: 'https://gitlab.com/',
                                requiresSSH: false,
                                requiresUsername: false,
                                supportsCommitSigning: false,
                                commitSigningConfiguration: null,
                            },
                            {
                                credential: {
                                    id: '123',
                                    isSiteCredential: true,
                                    sshPublicKey:
                                        'rsa-ssh randorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorando',
                                },
                                externalServiceKind: ExternalServiceKind.BITBUCKETSERVER,
                                externalServiceURL: 'https://bitbucket.sgdev.org/',
                                requiresSSH: true,
                                requiresUsername: false,
                                supportsCommitSigning: false,
                                commitSigningConfiguration: null,
                            },
                            {
                                credential: {
                                    id: '123',
                                    isSiteCredential: true,
                                    sshPublicKey:
                                        'rsa-ssh randorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorandorando',
                                },
                                externalServiceKind: ExternalServiceKind.BITBUCKETCLOUD,
                                externalServiceURL: 'https://bitbucket.org/',
                                requiresSSH: false,
                                requiresUsername: true,
                                supportsCommitSigning: false,
                                commitSigningConfiguration: null,
                            }
                        ),
                    ])
                }
            >
                <BatchChangesSiteConfigSettingsPage />
            </MockedTestProvider>
        )}
    </WebStory>
)

ConfigAdded.storyName = 'Config added'
