import { currentUserMock } from '@sourcegraph/shared/src/testing/integration/graphQlResults'

import { SourcegraphContext } from '../jscontext'

export const siteID = 'TestSiteID'
export const siteGQLID = 'TestGQLSiteID'

export const builtinAuthProvider = {
    serviceType: 'builtin' as const,
    serviceID: '',
    clientID: '',
    displayName: 'Builtin username-password authentication',
    isBuiltin: true,
    authenticationURL: '',
}

export const createJsContext = ({ sourcegraphBaseUrl }: { sourcegraphBaseUrl: string }): SourcegraphContext => ({
    currentUser: currentUserMock,
    externalURL: sourcegraphBaseUrl,
    accessTokensAllow: 'all-users-create',
    allowSignup: false,
    batchChangesEnabled: true,
    batchChangesDisableWebhooksWarning: false,
    batchChangesWebhookLogsEnabled: true,
    codeInsightsEnabled: true,
    executorsEnabled: true,
    codeIntelAutoIndexingEnabled: true,
    codeIntelAutoIndexingAllowGlobalPolicies: true,
    externalServicesUserMode: 'disabled',
    productResearchPageEnabled: true,
    assetsRoot: new URL('/.assets', sourcegraphBaseUrl).href,
    deployType: 'dev',
    debug: true,
    emailEnabled: false,
    experimentalFeatures: {},
    isAuthenticatedUser: true,
    likelyDockerOnMac: false,
    needServerRestart: false,
    needsSiteInit: false,
    resetPasswordEnabled: false,
    sentryDSN: null,
    site: {},
    siteID,
    siteGQLID,
    sourcegraphDotComMode: false,
    sourcegraphAppMode: false,
    userAgentIsBot: false,
    version: '0.0.0',
    xhrHeaders: {},
    authProviders: [builtinAuthProvider],
    authMinPasswordLength: 12,
})
