import type { DecoratorFn, Meta, Story } from '@storybook/react'
import { addMinutes, formatRFC3339 } from 'date-fns'

import { WebStory } from '../components/WebStory'
import { ExternalServiceKind, type WebhookFields } from '../graphql-operations'

import { WebhookInformation } from './WebhookInformation'

const decorator: DecoratorFn = Story => <Story />

const config: Meta = {
    title: 'web/site-admin/webhooks/incoming/WebhookInformation',
    decorators: [decorator],
}

export default config

const TIMESTAMP_MOCK = new Date(2021, 10, 8, 16, 40, 30)

export const WebhookDescription: Story = () => (
    <WebStory>{() => <WebhookInformation webhook={createWebhook()} />}</WebStory>
)

function createWebhook(): WebhookFields {
    return {
        __typename: 'Webhook',
        createdAt: formatRFC3339(TIMESTAMP_MOCK),
        id: '1',
        name: 'webhook with name',
        secret: 'secret-secret',
        updatedAt: formatRFC3339(addMinutes(TIMESTAMP_MOCK, 5)),
        url: 'sg.com/.api/webhooks/1aa2b42c-a14c-4aaa-b756-70c82e94d3e7',
        uuid: '1aa2b42c-a14c-4aaa-b756-70c82e94d3e7',
        codeHostKind: ExternalServiceKind.GITHUB,
        codeHostURN: 'github.com/repo1',
        createdBy: {
            username: 'alice',
            url: 'users/alice',
        },
        updatedBy: {
            username: 'alice',
            url: 'users/alice',
        },
    }
}

WebhookDescription.storyName = 'Webhook Information'
