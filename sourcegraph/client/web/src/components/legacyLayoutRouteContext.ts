import { of } from 'rxjs'

import { PlatformContext } from '@sourcegraph/shared/src/platform/context'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { NOOP_PLATFORM_CONTEXT } from '@sourcegraph/shared/src/testing/searchTestHelpers'

import {
    LegacyLayoutRouteContext,
    LegacyRouteComputedContext,
    LegacyRouteStaticInjections,
} from '../LegacyRouteContext'
import { DynamicSourcegraphWebAppContext, StaticSourcegraphWebAppContext } from '../SourcegraphWebApp'
import {
    StaticInjectedAppConfig,
    StaticHardcodedAppConfig,
    StaticWindowContextComputedAppConfig,
} from '../staticAppConfig'

const hardcodedConfig = {
    codeIntelligenceEnabled: true,
    codeInsightsEnabled: true,
    searchContextsEnabled: true,
    notebooksEnabled: true,
    codeMonitoringEnabled: true,
    searchAggregationEnabled: true,
    ownEnabled: true,
} satisfies StaticHardcodedAppConfig

export const windowContextConfig = {
    isSourcegraphDotCom: false,
    isSourcegraphApp: false,
    needsRepositoryConfiguration: false,
    batchChangesWebhookLogsEnabled: true,
    batchChangesEnabled: true,
} satisfies StaticWindowContextComputedAppConfig

export const injectedAppConfig = {} as unknown as StaticInjectedAppConfig

export const staticWebAppConfig = {
    setSelectedSearchContextSpec: () => {},
    platformContext: NOOP_PLATFORM_CONTEXT as PlatformContext,
    extensionsController: null,
} satisfies StaticSourcegraphWebAppContext

export const dynamicWebAppConfig = {
    selectedSearchContextSpec: '',
    authenticatedUser: null,
    settingsCascade: {
        final: null,
        subjects: null,
    },
    viewerSubject: {
        id: 'TEST_ID',
        viewerCanAdminister: false,
    },
} satisfies DynamicSourcegraphWebAppContext

export const legacyRouteComputedContext = {
    batchChangesExecutionEnabled: true,
    isMacPlatform: true,
} satisfies LegacyRouteComputedContext

export const legacyRouteInjectedContext = {
    getUserSearchContextNamespaces: () => [],
    fetchSearchContexts: () => of({}),
    fetchSearchContextBySpec: () => of({}),
    fetchSearchContext: () => of({}),
    createSearchContext: () => of({}),
    updateSearchContext: () => of({}),
    deleteSearchContext: () => of({}),
    isSearchContextSpecAvailable: () => of({}),
    streamSearch: () => of({}),
    fetchHighlightedFileLineRanges: () => of({}),
    telemetryService: NOOP_TELEMETRY_SERVICE,
} as Record<keyof LegacyRouteStaticInjections, unknown> as LegacyRouteStaticInjections

export const legacyLayoutRouteContextMock = {
    ...hardcodedConfig,
    ...windowContextConfig,
    ...injectedAppConfig,
    ...staticWebAppConfig,
    ...dynamicWebAppConfig,
    ...legacyRouteComputedContext,
    ...legacyRouteInjectedContext,
} satisfies LegacyLayoutRouteContext
