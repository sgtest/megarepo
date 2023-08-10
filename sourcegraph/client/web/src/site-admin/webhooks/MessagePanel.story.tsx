import type { DecoratorFn, Meta, Story } from '@storybook/react'

import { WebStory } from '../../components/WebStory'

import { MessagePanel } from './MessagePanel'
import { BODY_JSON, BODY_PLAIN, HEADERS_JSON, HEADERS_PLAIN } from './story/fixtures'

const decorator: DecoratorFn = story => <div className="p-3 container">{story()}</div>
const config: Meta = {
    title: 'web/site-admin/webhooks/MessagePanel',
    decorators: [decorator],
    parameters: {
        chromatic: {
            viewports: [576, 1440],
        },
    },
}

export default config

const messagePanelObject = {
    JSON: { headers: HEADERS_JSON, body: BODY_JSON },
    plain: { headers: HEADERS_PLAIN, body: BODY_PLAIN },
}

export const JSONRequest: Story = () => (
    <WebStory>
        {() => (
            <MessagePanel
                message={{
                    headers: messagePanelObject.JSON.headers,
                    body: messagePanelObject.JSON.body,
                }}
                requestOrStatusCode={{
                    method: 'POST',
                    url: '/my/url',
                    version: 'HTTP/1.1',
                }}
            />
        )}
    </WebStory>
)

JSONRequest.storyName = 'JSON request'

export const JSONResponse: Story = args => (
    <WebStory>
        {() => (
            <MessagePanel
                message={{
                    headers: messagePanelObject.JSON.headers,
                    body: messagePanelObject.JSON.body,
                }}
                requestOrStatusCode={args.requestOrStatusCode}
            />
        )}
    </WebStory>
)
JSONResponse.argTypes = {
    requestOrStatusCode: {
        control: { type: 'number', min: 100, max: 599 },
        defaultValue: 200,
    },
}

JSONResponse.storyName = 'JSON response'

export const PlainRequest: Story = () => (
    <WebStory>
        {() => (
            <MessagePanel
                message={{
                    headers: messagePanelObject.plain.headers,
                    body: messagePanelObject.plain.body,
                }}
                requestOrStatusCode={{
                    method: 'POST',
                    url: '/my/url',
                    version: 'HTTP/1.1',
                }}
            />
        )}
    </WebStory>
)

PlainRequest.storyName = 'plain request'

export const PlainResponse: Story = args => (
    <WebStory>
        {() => (
            <MessagePanel
                message={{
                    headers: messagePanelObject.plain.headers,
                    body: messagePanelObject.plain.body,
                }}
                requestOrStatusCode={args.requestOrStatusCode}
            />
        )}
    </WebStory>
)
PlainResponse.argTypes = {
    requestOrStatusCode: {
        control: { type: 'number', min: 100, max: 599 },
        defaultValue: 200,
    },
}

PlainResponse.storyName = 'plain response'
