import { AuthenticatedUser } from '../../../auth'
import { CodeMonitorFields, ListCodeMonitors } from '../../../graphql-operations'

export const mockUser: AuthenticatedUser = {
    __typename: 'User',
    id: 'userID',
    username: 'username',
    email: 'user@me.com',
    siteAdmin: true,
    databaseID: 0,
    tags: [],
    url: '',
    avatarURL: '',
    displayName: 'display name',
    settingsURL: '',
    viewerCanAdminister: true,
    organizations: {
        __typename: 'OrgConnection',
        nodes: [],
    },
    session: { __typename: 'Session', canSignOut: true },
    tosAccepted: true,
}

export const mockCodeMonitorFields: CodeMonitorFields = {
    __typename: 'Monitor',
    id: 'foo0',
    description: 'Test code monitor',
    enabled: true,
    trigger: { id: 'test-0', query: 'test' },
    actions: {
        nodes: [
            {
                __typename: 'MonitorEmail',
                id: 'test-action-0',
                enabled: true,
                includeResults: false,
                recipients: { nodes: [{ id: 'baz-0' }] },
            },
        ],
    },
}

export const mockCodeMonitor = {
    node: {
        __typename: 'Monitor',
        id: 'foo0',
        description: 'Test code monitor',
        enabled: true,
        owner: { id: 'test-id', namespaceName: 'test-user' },
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-0',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-0', url: '/user/test' }] },
                },
                {
                    __typename: 'MonitorSlackWebhook',
                    id: 'test-action-1',
                    enabled: true,
                    includeResults: false,
                    url: 'https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX',
                },
            ],
        },
        trigger: { id: 'test-0', query: 'test' },
    },
}

export const mockCodeMonitorNodes: ListCodeMonitors['nodes'] = [
    {
        id: 'foo0',
        description: 'Test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-0 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-0' }] },
                },
            ],
        },
        trigger: { id: 'test-0', query: 'test' },
    },
    {
        id: 'foo1',
        description: 'Second test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorSlackWebhook',
                    id: 'test-action-1 ',
                    enabled: true,
                    includeResults: false,
                    url: 'https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX',
                },
            ],
        },
        trigger: { id: 'test-1', query: 'test' },
    },
    {
        id: 'foo2',
        description: 'Third test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-2 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-2' }] },
                },
                {
                    __typename: 'MonitorSlackWebhook',
                    id: 'test-action-1 ',
                    enabled: true,
                    includeResults: false,
                    url: 'https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX',
                },
            ],
        },
        trigger: { id: 'test-2', query: 'test' },
    },
    {
        id: 'foo3',
        description: 'Fourth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-3 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-3' }] },
                },
                {
                    __typename: 'MonitorWebhook',
                    id: 'test-action-4',
                    enabled: true,
                    includeResults: false,
                    url: 'https://example.com/webhook',
                },
            ],
        },
        trigger: { id: 'test-3', query: 'test' },
    },
    {
        id: 'foo4',
        description: 'Fifth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorWebhook',
                    id: 'test-action-4',
                    enabled: true,
                    includeResults: false,
                    url: 'https://example.com/webhook',
                },
            ],
        },
        trigger: { id: 'test-4', query: 'test' },
    },
    {
        id: 'foo5',
        description: 'Sixth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-5 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-5' }] },
                },
            ],
        },
        trigger: { id: 'test-5', query: 'test' },
    },
    {
        id: 'foo6',
        description: 'Seventh test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-6 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-6' }] },
                },
            ],
        },
        trigger: { id: 'test-6', query: 'test' },
    },
    {
        id: 'foo7',
        description: 'Eighth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-7 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-7' }] },
                },
            ],
        },
        trigger: { id: 'test-7', query: 'test' },
    },
    {
        id: 'foo9',
        description: 'Ninth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-9 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-9' }] },
                },
            ],
        },
        trigger: { id: 'test-9', query: 'test' },
    },
    {
        id: 'foo10',
        description: 'Tenth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-0 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-0' }] },
                },
            ],
        },
        trigger: { id: 'test-0', query: 'test' },
    },
    {
        id: 'foo11',
        description: 'Eleventh test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-1 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-1' }] },
                },
            ],
        },
        trigger: { id: 'test-1', query: 'test' },
    },
    {
        id: 'foo12',
        description: 'Twelfth test code monitor',
        enabled: true,
        actions: {
            nodes: [
                {
                    __typename: 'MonitorEmail',
                    id: 'test-action-2 ',
                    enabled: true,
                    includeResults: false,
                    recipients: { nodes: [{ id: 'baz-2' }] },
                },
            ],
        },
        trigger: { id: 'test-2', query: 'test' },
    },
]

// Only minimal authenticated user data is needed for the code monitor tests
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions
export const mockAuthenticatedUser: AuthenticatedUser = {
    id: 'userID',
    username: 'username',
    email: 'user@me.com',
    siteAdmin: true,
} as AuthenticatedUser
