import { AuthenticatedUser } from '@sourcegraph/shared/src/auth'
import { SiteConfiguration } from '@sourcegraph/shared/src/schema/site.schema'
import { BatchChangesLicenseInfo } from '@sourcegraph/shared/src/testing/batches'

import { TemporarySettingsResult } from './graphql-operations'

export type DeployType = 'kubernetes' | 'docker-container' | 'docker-compose' | 'pure-docker' | 'dev' | 'helm'

/**
 * Defined in cmd/frontend/internal/app/jscontext/jscontext.go JSContext struct
 */

export interface AuthProvider {
    serviceType:
        | 'github'
        | 'gitlab'
        | 'bitbucketCloud'
        | 'http-header'
        | 'openidconnect'
        | 'sourcegraph-operator'
        | 'saml'
        | 'builtin'
        | 'gerrit'
        | 'azuredevops'
    displayName: string
    displayPrefix?: string
    isBuiltin: boolean
    authenticationURL: string
    serviceID: string
}

/**
 * This Typescript type should be in sync with client-side
 * GraphQL `CurrentAuthState` query.
 *
 * This type is derived from the generated `AuthenticatedUser` type.
 * It ensures that we don't forget to add new fields the server logic
 * if client side query changes.
 */
export type SourcegraphContextCurrentUser = Pick<
    AuthenticatedUser,
    | '__typename'
    | 'id'
    | 'databaseID'
    | 'username'
    | 'avatarURL'
    | 'displayName'
    | 'siteAdmin'
    | 'url'
    | 'settingsURL'
    | 'viewerCanAdminister'
    | 'tosAccepted'
    | 'searchable'
    | 'organizations'
    | 'session'
    | 'emails'
    | 'latestSettings'
    | 'permissions'
    | 'hasVerifiedEmail'
>

/**
 * This Typescript type should be in sync with client-side
 * GraphQL `GetTemporarySettings` query.
 *
 * This type is derived from the generated `TemporarySettingsResult` type.
 * It ensures that we don't forget to add new fields the server logic
 * if client side query changes.
 */
export type SourcegraphContextTemporarySettings = Pick<
    TemporarySettingsResult['temporarySettings'],
    '__typename' | 'contents'
>

export interface SourcegraphContext extends Pick<Required<SiteConfiguration>, 'experimentalFeatures'> {
    xhrHeaders: { [key: string]: string }
    userAgentIsBot: boolean

    /**
     * Whether the user is authenticated. Use authenticatedUser in ./auth.ts to obtain information about the user.
     */
    readonly isAuthenticatedUser: boolean

    /**
     * Data preloaded on the server.
     */
    readonly currentUser: SourcegraphContextCurrentUser | null
    readonly temporarySettings: SourcegraphContextTemporarySettings | null

    readonly sentryDSN: string | null

    readonly openTelemetry?: {
        endpoint: string
    }

    /** Externally accessible URL for Sourcegraph (e.g., https://sourcegraph.com or http://localhost:3080). */
    externalURL: string

    /** Whether instance allows to change its settings manually in UI */
    extsvcConfigAllowEdits: boolean

    /** Whether instance allows is configured by external service configuration file */
    extsvcConfigFileExists: boolean

    /** URL path to image/font/etc. assets on server */
    assetsRoot: string

    version: string

    /**
     * Debug is whether debug mode is enabled.
     */
    debug: boolean

    sourcegraphDotComMode: boolean
    sourcegraphAppMode: boolean

    /**
     * siteID is the identifier of the Sourcegraph site.
     */
    siteID: string

    /** The GraphQL ID of the Sourcegraph site. */
    siteGQLID: string

    /**
     * Whether the site needs to be initialized.
     */
    needsSiteInit: boolean

    /**
     * Whether at least one code host connections needs to be connected.
     */
    needsRepositoryConfiguration: boolean

    /**
     * Emails support enabled
     */
    emailEnabled: boolean

    /**
     * A subset of the site configuration. Not all fields are set.
     */
    site: Pick<SiteConfiguration, 'auth.public' | 'update.channel' | 'authz.enforceForSiteAdmins'>

    /** Whether access tokens are enabled. */
    accessTokensAllow: 'all-users-create' | 'site-admin-create' | 'none'

    /** Whether the reset-password flow is enabled. */
    resetPasswordEnabled: boolean

    /** Whether the instance is running on macOS. */
    runningOnMacOS: boolean

    /**
     * Whether or not the server needs to restart in order to apply a pending
     * configuration change.
     */
    needServerRestart: boolean

    /**
     * The kind of deployment.
     */
    deployType: DeployType

    /** Whether signup is allowed on the site. */
    allowSignup: boolean

    /** Whether the batch changes feature is enabled on the site. */
    batchChangesEnabled: boolean

    /** Whether the warning about unconfigured webhooks is disabled within Batch
     * Changes. */
    batchChangesDisableWebhooksWarning: boolean

    batchChangesWebhookLogsEnabled: boolean

    /** Whether cody is enabled for the user. */
    codyEnabled: boolean

    /** Whether the site requires a verified email for cody. */
    codyRequiresVerifiedEmail: boolean

    /** Whether executors are enabled on the site. */
    executorsEnabled: boolean

    /** Whether the code intel auto-indexer feature is enabled on the site. */
    codeIntelAutoIndexingEnabled: boolean

    /** Whether global policies are enabled for auto-indexing. */
    codeIntelAutoIndexingAllowGlobalPolicies: boolean

    /** Whether code insights API is enabled on the site. */
    codeInsightsEnabled: boolean

    /** Whether embeddings are enabled on this site. */
    embeddingsEnabled: boolean

    /**
     * Local git URL, it's used only to create a local external service
     * in Sourcegraph App.
     */
    srcServeGitUrl: string

    /** Whether users are allowed to add their own code and at what permission level. */
    externalServicesUserMode: 'disabled' | 'public' | 'all' | 'unknown'

    /** Authentication provider instances in site config. */
    authProviders: AuthProvider[]

    /** primaryLoginProvidersCount sets the max number of primary login providers on signin page */
    primaryLoginProvidersCount: number

    /** What the minimum length for a password should be. */
    authMinPasswordLength: number

    authPasswordPolicy?: {
        /** Whether password policy is enabled or not */
        enabled?: boolean

        /** Mandatory amount of special characters in password */
        numberOfSpecialCharacters?: number

        /** Require at least one number in password */
        requireAtLeastOneNumber?: boolean

        /** Require at least an upper and a lowercase character password */
        requireUpperandLowerCase?: boolean
    }

    authAccessRequest?: SiteConfiguration['auth.accessRequest']

    /** Custom branding for the homepage and search icon. */
    branding?: {
        /** The URL of the favicon to be used for your instance */
        favicon?: string

        /** Override style for light themes */
        light?: BrandAssets
        /** Override style for dark themes */
        dark?: BrandAssets

        /** Prevents the icon in the top-left corner of the screen from spinning. */
        disableSymbolSpin?: boolean

        brandName: string
    }

    /** Whether the product research sign-up page is enabled on the site. */
    productResearchPageEnabled: boolean

    /** Contains information about the product license. */
    licenseInfo?: {
        currentPlan: 'old-starter-0' | 'old-enterprise-0' | 'team-0' | 'enterprise-0' | 'business-0' | 'enterprise-1'

        codeScaleLimit?: string
        codeScaleCloseToLimit?: boolean
        codeScaleExceededLimit?: boolean
        batchChanges?: BatchChangesLicenseInfo
        knownLicenseTags?: string[]
    }

    /** Prompt users with browsers that would crash to download a modern browser. */
    RedirectUnsupportedBrowser?: boolean

    outboundRequestLogLimit?: number

    /** Whether the feedback survey is enabled. */
    disableFeedbackSurvey?: boolean
}

export interface BrandAssets {
    /** The URL to the logo used on the homepage */
    logo?: string
    /** The URL to the symbol used as the search icon */
    symbol?: string
}
