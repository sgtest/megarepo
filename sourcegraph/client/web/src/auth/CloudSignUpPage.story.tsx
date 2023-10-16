import type { Meta, StoryFn } from '@storybook/react'
import sinon from 'sinon'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { WebStory } from '../components/WebStory'
import type { SourcegraphContext } from '../jscontext'

import { CloudSignUpPage } from './CloudSignUpPage'

const config: Meta = {
    title: 'web/auth/CloudSignUpPage',
    parameters: {
        chromatic: { disableSnapshot: false },
    },
}

export default config

const context: Pick<SourcegraphContext, 'authProviders' | 'experimentalFeatures' | 'authMinPasswordLength'> = {
    authProviders: [
        {
            clientID: '000',
            serviceType: 'github',
            displayName: 'GitHub.com',
            isBuiltin: false,
            authenticationURL: '/.auth/github/login?pc=https%3A%2F%2Fgithub.com%2F',
            serviceID: 'https://github.com',
        },
        {
            clientID: '001',
            serviceType: 'gitlab',
            displayName: 'GitLab.com',
            isBuiltin: false,
            authenticationURL: '/.auth/gitlab/login?pc=https%3A%2F%2Fgitlab.com%2F',
            serviceID: 'https://gitlab.com',
        },
        {
            clientID: '002',
            serviceType: 'openidconnect',
            displayName: 'Google',
            isBuiltin: false,
            authenticationURL: '/.auth/openidconnect/login?pc=google',
            serviceID: 'https://gitlab.com',
        },
    ],
    experimentalFeatures: {},
    authMinPasswordLength: 0,
}

export const Default: StoryFn = () => (
    <WebStory>
        {({ isLightTheme }) => (
            <CloudSignUpPage
                isLightTheme={isLightTheme}
                source="Monitor"
                onSignUp={sinon.stub()}
                context={context}
                showEmailForm={false}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                isSourcegraphDotCom={true}
            />
        )}
    </WebStory>
)

export const EmailForm: StoryFn = () => (
    <WebStory>
        {({ isLightTheme }) => (
            <CloudSignUpPage
                isLightTheme={isLightTheme}
                source="SearchCTA"
                onSignUp={sinon.stub()}
                context={context}
                showEmailForm={true}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                isSourcegraphDotCom={true}
            />
        )}
    </WebStory>
)

export const InvalidSource: StoryFn = () => (
    <WebStory>
        {({ isLightTheme }) => (
            <CloudSignUpPage
                isLightTheme={isLightTheme}
                source="test"
                onSignUp={sinon.stub()}
                context={context}
                showEmailForm={false}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                isSourcegraphDotCom={true}
            />
        )}
    </WebStory>
)

export const OptimizationSignup: StoryFn = () => (
    <WebStory>
        {({ isLightTheme }) => (
            <CloudSignUpPage
                isLightTheme={isLightTheme}
                source="test"
                onSignUp={sinon.stub()}
                context={context}
                showEmailForm={false}
                telemetryService={NOOP_TELEMETRY_SERVICE}
                isSourcegraphDotCom={true}
            />
        )}
    </WebStory>
)
