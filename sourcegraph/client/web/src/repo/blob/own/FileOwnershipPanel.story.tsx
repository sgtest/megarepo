import { MockedResponse } from '@apollo/client/testing'
import { Meta, Story } from '@storybook/react'

import { getDocumentNode } from '@sourcegraph/http-client'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { WebStory } from '../../../components/WebStory'
import { FetchOwnershipResult } from '../../../graphql-operations'

import { FileOwnershipPanel } from './FileOwnershipPanel'
import { FETCH_OWNERS } from './grapqlQueries'

const response: FetchOwnershipResult = {
    node: {
        __typename: 'Repository',
        commit: {
            blob: {
                ownership: {
                    totalOwners: 4,
                    nodes: [
                        {
                            __typename: 'Ownership',
                            owner: {
                                __typename: 'Person',
                                email: 'alice@example.com',
                                avatarURL: null,
                                displayName: '',
                                user: null,
                            },
                            reasons: [
                                {
                                    __typename: 'CodeownersFileEntry',
                                    title: 'CodeOwner',
                                    description: 'This person is listed in the CODEOWNERS file',
                                    codeownersFile: {
                                        __typename: 'VirtualFile',
                                        url: '/own',
                                    },
                                    ruleLineMatch: 10,
                                },
                            ],
                        },
                        {
                            __typename: 'Ownership',
                            owner: {
                                __typename: 'Person',
                                email: 'bob@example.com',
                                avatarURL: 'https://avatars.githubusercontent.com/u/5090588?v=4',
                                displayName: 'Bob the Builder',
                                user: {
                                    __typename: 'User',
                                    displayName: 'Bob the Builder',
                                    url: '/users/bob',
                                    username: 'bob',
                                    primaryEmail: {
                                        __typename: 'UserEmail',
                                        email: 'bob-primary@example.com',
                                    },
                                },
                            },
                            reasons: [
                                {
                                    __typename: 'CodeownersFileEntry',
                                    title: 'CodeOwner',
                                    description: 'This person is listed in the CODEOWNERS file',
                                    codeownersFile: {
                                        __typename: 'VirtualFile',
                                        url: '/own',
                                    },
                                    ruleLineMatch: 10,
                                },
                                {
                                    __typename: 'RecentContributorOwnershipSignal',
                                    title: 'Recent Contributor',
                                    description:
                                        'Owner is associated because they have contributed to this file in the last 90 days',
                                },
                            ],
                        },
                        {
                            __typename: 'Ownership',
                            owner: {
                                __typename: 'Team',
                                avatarURL: null,
                                teamDisplayName: 'Delta Team',
                                name: 'delta',
                                external: false,
                                url: '/teams/delta',
                            },
                            reasons: [
                                {
                                    __typename: 'CodeownersFileEntry',
                                    title: 'CodeOwner',
                                    description: 'This team is listed in the CODEOWNERS file',
                                    codeownersFile: {
                                        __typename: 'VirtualFile',
                                        url: '/own',
                                    },
                                    ruleLineMatch: 10,
                                },
                            ],
                        },
                        {
                            __typename: 'Ownership',
                            owner: {
                                __typename: 'Person',
                                email: '',
                                avatarURL: null,
                                displayName: 'charlie',
                                user: null,
                            },
                            reasons: [
                                {
                                    __typename: 'RecentContributorOwnershipSignal',
                                    title: 'Recent Contributor',
                                    description:
                                        'Owner is associated because they have contributed to this file in the last 90 days',
                                },
                                {
                                    __typename: 'RecentViewOwnershipSignal',
                                    title: 'Recent View',
                                    description:
                                        'Owner is associated because they have viewed this file in the last 90 days.',
                                },
                            ],
                        },
                        {
                            __typename: 'Ownership',
                            owner: {
                                __typename: 'Person',
                                email: '',
                                avatarURL: null,
                                displayName: 'alice',
                                user: null,
                            },
                            reasons: [
                                {
                                    __typename: 'RecentViewOwnershipSignal',
                                    title: 'Recent View',
                                    description:
                                        'Owner is associated because they have viewed this file in the last 90 days.',
                                },
                            ],
                        },
                    ],
                },
            },
        },
    },
}

const mockResponse: MockedResponse<FetchOwnershipResult> = {
    request: {
        query: getDocumentNode(FETCH_OWNERS),
        variables: {
            repo: 'github.com/sourcegraph/sourcegraph',
            currentPath: 'README.md',
            revision: '',
        },
    },
    result: {
        data: response,
    },
}

const config: Meta = {
    title: 'web/repo/blob/own/FileOwnership',
    parameters: {
        chromatic: { disableSnapshot: false },
    },
}

export default config

export const Default: Story = () => (
    <WebStory mocks={[mockResponse]}>
        {() => (
            <FileOwnershipPanel
                repoID="github.com/sourcegraph/sourcegraph"
                filePath="README.md"
                telemetryService={NOOP_TELEMETRY_SERVICE}
            />
        )}
    </WebStory>
)
