import { Meta, Story, DecoratorFn } from '@storybook/react'

import { ExternalServiceKind } from '@sourcegraph/shared/src/graphql-operations'
import { BatchSpecSource } from '@sourcegraph/shared/src/schema'

import { WebStory } from '../../../components/WebStory'

import { WebhookAlert } from './WebhookAlert'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>

const config: Meta = {
    title: 'web/batches/details/WebhookAlert',
    decorators: [decorator],
}

export default config

const id = new Date().toString()

const currentSpec = {
    id: 'specID1',
    originalInput: '',
    supersedingBatchSpec: null,
    source: BatchSpecSource.REMOTE,
}

const batchChange = (totalCount: number, hasNextPage: boolean) => ({
    id,
    currentSpec: {
        ...currentSpec,
        codeHostsWithoutWebhooks: {
            nodes: [
                {
                    externalServiceKind: 'GITHUB' as ExternalServiceKind,
                    externalServiceURL: 'https://github.com/',
                },
                {
                    externalServiceKind: 'GITLAB' as ExternalServiceKind,
                    externalServiceURL: 'https://gitlab.com/',
                },
                {
                    externalServiceKind: 'BITBUCKETSERVER' as ExternalServiceKind,
                    externalServiceURL: 'https://bitbucket.com/',
                },
            ],
            pageInfo: { hasNextPage },
            totalCount,
        },
    },
})

export const SiteAdmin: Story = () => (
    <WebStory>{() => <WebhookAlert batchChange={batchChange(3, false)} isSiteAdmin={true} />}</WebStory>
)

SiteAdmin.storyName = 'Site admin'

export const RegularUser: Story = () => (
    <WebStory>{() => <WebhookAlert batchChange={batchChange(3, false)} />}</WebStory>
)

RegularUser.storyName = 'Regular user'

export const RegularUserWithMoreThanThreeCodeHosts: Story = () => (
    <WebStory>{() => <WebhookAlert batchChange={batchChange(4, true)} />}</WebStory>
)

RegularUserWithMoreThanThreeCodeHosts.storyName = 'Regular user with more than three code hosts'

export const AllCodeHostsHaveWebhooks: Story = () => (
    <WebStory>
        {() => (
            <WebhookAlert
                batchChange={{
                    id,
                    currentSpec: {
                        ...currentSpec,
                        codeHostsWithoutWebhooks: {
                            nodes: [],
                            pageInfo: { hasNextPage: false },
                            totalCount: 0,
                        },
                    },
                }}
            />
        )}
    </WebStory>
)

AllCodeHostsHaveWebhooks.storyName = 'All code hosts have webhooks'
