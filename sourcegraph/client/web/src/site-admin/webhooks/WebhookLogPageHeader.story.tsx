import React, { useState } from 'react'

import { DecoratorFn, Meta, Story } from '@storybook/react'

import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'
import { Container } from '@sourcegraph/wildcard'

import { WebStory } from '../../components/WebStory'

import { SelectedExternalService } from './backend'
import { buildHeaderMock } from './story/fixtures'
import { WebhookLogPageHeader } from './WebhookLogPageHeader'

const decorator: DecoratorFn = story => (
    <Container>
        <div className="p-3 container">{story()}</div>
    </Container>
)

const config: Meta = {
    title: 'web/site-admin/webhooks/WebhookLogPageHeader',
    parameters: {
        chromatic: {
            viewports: [320, 576, 978, 1440],
        },
    },
    decorators: [decorator],
    argTypes: {
        externalServiceCount: {
            name: 'external service count',
            control: { type: 'number' },
        },
        erroredWebhookCount: {
            name: 'errored webhook count',
            control: { type: 'number' },
        },
    },
}

export default config

// Create a component to handle the minimum state management required for a
// WebhookLogPageHeader.
const WebhookLogPageHeaderContainer: React.FunctionComponent<
    React.PropsWithChildren<{
        initialExternalService?: SelectedExternalService
        initialOnlyErrors?: boolean
    }>
> = ({ initialExternalService, initialOnlyErrors }) => {
    const [onlyErrors, setOnlyErrors] = useState(initialOnlyErrors === true)
    const [externalService, setExternalService] = useState(initialExternalService ?? 'all')

    return (
        <WebhookLogPageHeader
            externalService={externalService}
            onlyErrors={onlyErrors}
            onSelectExternalService={setExternalService}
            onSetOnlyErrors={setOnlyErrors}
        />
    )
}

export const AllZeroes: Story = args => (
    <WebStory>
        {() => (
            <MockedTestProvider mocks={buildHeaderMock(args.externalServiceCount, args.erroredWebhookCount)}>
                <WebhookLogPageHeaderContainer />
            </MockedTestProvider>
        )}
    </WebStory>
)
AllZeroes.argTypes = {
    externalServiceCount: {
        defaultValue: 0,
    },
    erroredWebhookCount: {
        defaultValue: 0,
    },
}

AllZeroes.storyName = 'all zeroes'

export const ExternalServices: Story = args => (
    <WebStory>
        {() => (
            <MockedTestProvider mocks={buildHeaderMock(args.externalServiceCount, args.erroredWebhookCount)}>
                <WebhookLogPageHeaderContainer />
            </MockedTestProvider>
        )}
    </WebStory>
)

ExternalServices.argTypes = {
    externalServiceCount: {
        defaultValue: 10,
    },
    erroredWebhookCount: {
        defaultValue: 0,
    },
}

ExternalServices.storyName = 'external services'

export const ExternalServicesAndErrors: Story = args => (
    <WebStory>
        {() => (
            <MockedTestProvider mocks={buildHeaderMock(args.externalServiceCount, args.erroredWebhookCount)}>
                <WebhookLogPageHeaderContainer />
            </MockedTestProvider>
        )}
    </WebStory>
)

ExternalServicesAndErrors.argTypes = {
    externalServiceCount: {
        defaultValue: 20,
    },
    erroredWebhookCount: {
        defaultValue: 500,
    },
}

ExternalServicesAndErrors.storyName = 'external services and errors'

export const OnlyErrorsTurnedOn: Story = args => (
    <WebStory>
        {() => (
            <MockedTestProvider mocks={buildHeaderMock(args.externalServiceCount, args.erroredWebhookCount)}>
                <WebhookLogPageHeaderContainer initialOnlyErrors={true} />
            </MockedTestProvider>
        )}
    </WebStory>
)
OnlyErrorsTurnedOn.argTypes = {
    externalServiceCount: {
        defaultValue: 20,
    },
    erroredWebhookCount: {
        defaultValue: 500,
    },
}

OnlyErrorsTurnedOn.storyName = 'only errors turned on'

export const SpecificExternalServiceSelected: Story = args => (
    <WebStory>
        {() => (
            <MockedTestProvider mocks={buildHeaderMock(args.externalServiceCount, args.erroredWebhookCount)}>
                <WebhookLogPageHeaderContainer initialExternalService={args.initialExternalService.toString()} />
            </MockedTestProvider>
        )}
    </WebStory>
)
SpecificExternalServiceSelected.argTypes = {
    initialExternalService: {
        control: { type: 'number', min: 0, max: 19 },
        defaultValue: 2,
    },
    externalServiceCount: {
        defaultValue: 20,
    },
    erroredWebhookCount: {
        defaultValue: 500,
    },
}

SpecificExternalServiceSelected.storyName = 'specific external service selected'

export const UnmatchedExternalServiceSelected: Story = args => (
    <WebStory>
        {() => (
            <MockedTestProvider mocks={buildHeaderMock(args.externalServiceCount, args.erroredWebhookCount)}>
                <WebhookLogPageHeaderContainer initialExternalService="unmatched" />
            </MockedTestProvider>
        )}
    </WebStory>
)

UnmatchedExternalServiceSelected.argTypes = {
    externalServiceCount: {
        defaultValue: 20,
    },
    erroredWebhookCount: {
        defaultValue: 500,
    },
}

UnmatchedExternalServiceSelected.storyName = 'unmatched external service selected'
