import { FC, PropsWithChildren, createContext, useContext, useCallback } from 'react'

import { Observable } from 'rxjs'

import { isMacPlatform } from '@sourcegraph/common'
import { FetchFileParameters, fetchHighlightedFileLineRanges } from '@sourcegraph/shared/src/backend/file'
import { PlatformContext } from '@sourcegraph/shared/src/platform/context'
import {
    fetchSearchContextBySpec,
    fetchSearchContexts,
    fetchSearchContext,
    getUserSearchContextNamespaces,
    createSearchContext,
    updateSearchContext,
    deleteSearchContext,
    isSearchContextSpecAvailable,
    SearchContextProps,
} from '@sourcegraph/shared/src/search'
import { aggregateStreamingSearch } from '@sourcegraph/shared/src/search/stream'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { globbingEnabledFromSettings } from '@sourcegraph/shared/src/util/globbing'

import { isBatchChangesExecutionEnabled } from './batches'
import { useBreadcrumbs, BreadcrumbSetters, BreadcrumbsProps } from './components/Breadcrumbs'
import { SearchStreamingProps } from './search'
import { StaticSourcegraphWebAppContext, DynamicSourcegraphWebAppContext } from './SourcegraphWebApp'
import { StaticAppConfig } from './staticAppConfig'
import { eventLogger } from './tracking/eventLogger'

export interface StaticLegacyRouteContext extends LegacyRouteComputedContext, LegacyRouteStaticInjections {}

/**
 * Static values we compute in the `<LegacyRoute /> component.
 *
 * Static in the sense that there are no other ways to change
 * these values except by refetching the entire original value (settingsCascade)
 */
export interface LegacyRouteComputedContext {
    /**
     * TODO: expose these fields in the new `useSettings()` hook, calculate next to source.
     */
    globbing: boolean
    batchChangesExecutionEnabled: boolean

    /**
     * TODO: remove from the context and repalce with isMacPlatform() calls
     */
    isMacPlatform: boolean
}

/**
 * Non-primitive values (components, objects) we inject in the <LegacyRoute /> component.
 *
 * TODO: consolidate all static intejections in one place or get rid of them is possible.
 */
export interface LegacyRouteStaticInjections
    extends Pick<TelemetryProps, 'telemetryService'>,
        Pick<
            SearchContextProps,
            | 'getUserSearchContextNamespaces'
            | 'fetchSearchContexts'
            | 'fetchSearchContextBySpec'
            | 'fetchSearchContext'
            | 'createSearchContext'
            | 'updateSearchContext'
            | 'deleteSearchContext'
            | 'isSearchContextSpecAvailable'
        >,
        Pick<SearchStreamingProps, 'streamSearch'>,
        Pick<BreadcrumbsProps, 'breadcrumbs'>,
        Pick<BreadcrumbSetters, 'useBreadcrumb' | 'setBreadcrumb'> {
    // Search
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

/**
 * LegacyRoute component props consist of fours parts:
 * 1. StaticAppConfig — injected at the tip of the React tree.
 * 2. StaticSourcegraphWebAppContext — injected by the `SourcegraphWebApp` component.
 * 3. DynamicSourcegraphWebAppContext — injected by the `SourcegraphWebApp` component.
 * 4. StaticLegacyRouteContext — injected by the `StaticLegacyRouteContext` component
 *
 * All these fields are static except for `DynamicSourcegraphWebAppContext`.
 */
export interface LegacyLayoutRouteContext
    extends StaticAppConfig,
        StaticSourcegraphWebAppContext,
        DynamicSourcegraphWebAppContext,
        StaticLegacyRouteContext {}

interface LegacyRouteProps {
    render: (props: LegacyLayoutRouteContext) => JSX.Element
    condition?: (props: LegacyLayoutRouteContext) => boolean
}

/**
 * A wrapper component for React router route entrypoints that still need access to the legacy
 * route context and prop drilling.
 */
export const LegacyRoute: FC<LegacyRouteProps> = ({ render, condition }) => {
    const context = useContext(LegacyRouteContext)
    if (!context) {
        throw new Error('LegacyRoute must be used inside a LegacyRouteContext.Provider')
    }

    if (condition && !condition(context)) {
        return null
    }

    return render(context)
}

export interface LegacyRouteContextProviderProps {
    context: StaticAppConfig & StaticSourcegraphWebAppContext & DynamicSourcegraphWebAppContext
}

export const LegacyRouteContextProvider: FC<PropsWithChildren<LegacyRouteContextProviderProps>> = props => {
    const { children, context } = props
    const { settingsCascade, platformContext } = context

    const _fetchHighlightedFileLineRanges = useCallback(
        (parameters: FetchFileParameters, force?: boolean | undefined): Observable<string[][]> =>
            fetchHighlightedFileLineRanges({ ...parameters, platformContext }, force),
        [platformContext]
    )

    const breadcrumbProps = useBreadcrumbs()

    const injections = {
        /**
         * Search context props
         */
        getUserSearchContextNamespaces,
        fetchSearchContexts,
        fetchSearchContextBySpec,
        fetchSearchContext,
        createSearchContext,
        updateSearchContext,
        deleteSearchContext,
        isSearchContextSpecAvailable,

        /**
         * Other injections from static imports
         */
        streamSearch: aggregateStreamingSearch,
        fetchHighlightedFileLineRanges: _fetchHighlightedFileLineRanges,
        telemetryService: eventLogger,

        /**
         * Breadcrumb props
         */
        ...breadcrumbProps,
    } satisfies LegacyRouteStaticInjections

    const computedContextFields = {
        globbing: globbingEnabledFromSettings(settingsCascade),
        batchChangesExecutionEnabled: isBatchChangesExecutionEnabled(settingsCascade),
        isMacPlatform: isMacPlatform(),
    } satisfies LegacyRouteComputedContext

    const legacyContext = {
        ...context,
        ...injections,
        ...computedContextFields,
    } satisfies LegacyLayoutRouteContext

    return <LegacyRouteContext.Provider value={legacyContext}>{children}</LegacyRouteContext.Provider>
}

export const LegacyRouteContext = createContext<LegacyLayoutRouteContext | null>(null)

/**
 * A convenience hook to return the platform context.
 *
 * @deprecated This should not be used for new code anymore, please use Apollo instead to make
 * GraphQL requests and `useSettings` to access settings.
 */
export const useLegacyPlatformContext = (): PlatformContext => {
    const context = useContext(LegacyRouteContext)
    if (!context) {
        throw new Error('LegacyRoute must be used inside a LegacyRouteContext.Provider')
    }
    return context.platformContext
}
