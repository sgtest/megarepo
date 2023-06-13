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

    const jsContext: SourcegraphContext = {
        currentUser: null,
        temporarySettings: null,
        externalURL: sourcegraphBaseUrl,
        accessTokensAllow: 'all-users-create',
        allowSignup: true,
        batchChangesEnabled: true,
        batchChangesDisableWebhooksWarning: false,
        batchChangesWebhookLogsEnabled: true,
        executorsEnabled: false,
        codyEnabled: true,
        codyEnabledForCurrentUser: true,
        codyRequiresVerifiedEmail: false,
        codeIntelAutoIndexingEnabled: false,
        codeIntelAutoIndexingAllowGlobalPolicies: false,
        codeInsightsEnabled: true,
        externalServicesUserMode: 'public',
        productResearchPageEnabled: true,
        assetsRoot: '/.assets',
        deployType: 'dev',
        debug: true,
        emailEnabled: false,
        experimentalFeatures: {},
        extsvcConfigAllowEdits: false,
        extsvcConfigFileExists: false,
        isAuthenticatedUser: true,
        needServerRestart: false,
        needsSiteInit: false,
        needsRepositoryConfiguration: false,
        resetPasswordEnabled: true,
        runningOnMacOS: true,
        sentryDSN: null,
        site: {
            'update.channel': 'release',
        },
        siteID: 'TestSiteID',
        siteGQLID: 'TestGQLSiteID',
        sourcegraphDotComMode: ENVIRONMENT_CONFIG.SOURCEGRAPHDOTCOM_MODE,
        sourcegraphAppMode: false,
        srcServeGitUrl: 'http://127.0.0.1:3434',
        userAgentIsBot: false,
        version: '0.0.0',
        xhrHeaders: {},
        authProviders: [builtinAuthProvider],
        authMinPasswordLength: 12,
        authPasswordPolicy: {
            enabled: false,
            numberOfSpecialCharacters: 2,
            requireAtLeastOneNumber: true,
            requireUpperandLowerCase: true,
        },
        openTelemetry: {
            endpoint: ENVIRONMENT_CONFIG.CLIENT_OTEL_EXPORTER_OTLP_ENDPOINT,
        },
        embeddingsEnabled: false,
        primaryLoginProvidersCount: 5,
        // Site-config overrides default JS context
        ...siteConfig,
    }

    return jsContext
}
