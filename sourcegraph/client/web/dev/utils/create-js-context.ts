import { SourcegraphContext } from '../../src/jscontext'

import { ENVIRONMENT_CONFIG } from './environment-config'
import { getSiteConfig } from './get-site-config'

// TODO: share with `client/web/src/integration/jscontext` which is not included into `tsconfig.json` now.
export const builtinAuthProvider = {
    serviceType: 'builtin' as const,
    serviceID: '',
    clientID: '',
    displayName: 'Builtin username-password authentication',
    isBuiltin: true,
    authenticationURL: '',
}

// Create dummy JS context that will be added to index.html when `WEBPACK_SERVE_INDEX` is set to true.
export const createJsContext = ({ sourcegraphBaseUrl }: { sourcegraphBaseUrl: string }): SourcegraphContext => {
    const siteConfig = getSiteConfig()

    if (siteConfig?.authProviders) {
        siteConfig.authProviders.unshift(builtinAuthProvider)
    }

    return {
        externalURL: sourcegraphBaseUrl,
        accessTokensAllow: 'all-users-create',
        allowSignup: true,
        batchChangesEnabled: true,
        batchChangesDisableWebhooksWarning: false,
        batchChangesWebhookLogsEnabled: true,
        executorsEnabled: false,
        codeIntelAutoIndexingEnabled: false,
        codeIntelAutoIndexingAllowGlobalPolicies: false,
        codeIntelLockfileIndexingEnabled: false,
        codeInsightsGqlApiEnabled: true,
        externalServicesUserMode: 'public',
        productResearchPageEnabled: true,
        assetsRoot: '/.assets',
        deployType: 'dev',
        debug: true,
        emailEnabled: false,
        experimentalFeatures: {},
        isAuthenticatedUser: true,
        likelyDockerOnMac: false,
        needServerRestart: false,
        needsSiteInit: false,
        resetPasswordEnabled: true,
        sentryDSN: null,
        site: {
            'update.channel': 'release',
        },
        siteID: 'TestSiteID',
        siteGQLID: 'TestGQLSiteID',
        sourcegraphDotComMode: ENVIRONMENT_CONFIG.SOURCEGRAPHDOTCOM_MODE,
        githubAppCloudSlug: 'TestApp',
        githubAppCloudClientID: 'TestClientID',
        userAgentIsBot: false,
        version: '0.0.0',
        xhrHeaders: {},
        authProviders: [builtinAuthProvider],
        // Site-config overrides default JS context
        ...siteConfig,
    }
}
