import { GraphQLResult } from '../../../shared/src/graphql/graphql'
import * as GQL from '../../../shared/src/graphql/schema'
import { ExtensionHoverAlertType } from '../libs/code_intelligence/hover_alerts'
import { DEFAULT_SOURCEGRAPH_URL } from '../shared/util/context'

interface RepoLocations {
    [key: string]: string
}

export interface PhabricatorMapping {
    callsign: string
    path: string
}

/**
 * The feature flags available.
 */
export interface FeatureFlags {
    /**
     * Allow error reporting.
     *
     * @duration permanent
     * @todo Since this is not really a feature flag, just unnest it into settings (and potentially get rid of the feature flags abstraction completely)
     */
    allowErrorReporting: boolean

    /**
     * Support link previews from extensions in content views (such as GitHub issues).
     */
    experimentalLinkPreviews: boolean

    /**
     * Support completion in text fields (such as on GitHub issues).
     */
    experimentalTextFieldCompletion: boolean
}

export const featureFlagDefaults: FeatureFlags = {
    allowErrorReporting: false,
    experimentalLinkPreviews: false,
    experimentalTextFieldCompletion: false,
}

export interface StorageItems {
    sourcegraphURL: string

    identity: string
    enterpriseUrls: string[]
    repoLocations: RepoLocations
    phabricatorMappings: PhabricatorMapping[]
    sourcegraphAnonymousUid: string
    disableExtension: boolean
    /**
     * Storage for feature flags.
     */
    featureFlags: Partial<FeatureFlags>
    clientConfiguration: ClientConfigurationDetails
    /**
     * Overrides settings from Sourcegraph.
     */
    clientSettings: string
    sideloadedExtensionURL: string | null
    dismissedHoverAlerts: {
        [alertType in ExtensionHoverAlertType]?: boolean
    }
}

interface ClientConfigurationDetails {
    contentScriptUrls: string[]
    parentSourcegraph: {
        url: string
    }
}

export const defaultStorageItems: StorageItems = {
    sourcegraphURL: DEFAULT_SOURCEGRAPH_URL,

    identity: '',
    enterpriseUrls: [],
    repoLocations: {},
    phabricatorMappings: [],
    sourcegraphAnonymousUid: '',
    disableExtension: false,
    featureFlags: featureFlagDefaults,
    clientConfiguration: {
        contentScriptUrls: [],
        parentSourcegraph: {
            url: DEFAULT_SOURCEGRAPH_URL,
        },
    },
    clientSettings: '',
    sideloadedExtensionURL: null,
    dismissedHoverAlerts: {},
}

/**
 * Functions in the background page that can be invoked from content scripts.
 */
export interface BackgroundMessageHandlers {
    openOptionsPage(): Promise<void>
    createBlobURL(bundleUrl: string): Promise<string>
    requestGraphQL<T extends GQL.IQuery | GQL.IMutation>(options: {
        request: string
        variables: {}
    }): Promise<GraphQLResult<T>>
}
