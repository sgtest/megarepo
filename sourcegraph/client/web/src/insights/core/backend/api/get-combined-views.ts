import { Remote } from 'comlink'
import { combineLatest, from, Observable } from 'rxjs'
import { map, switchMap } from 'rxjs/operators'

import { wrapRemoteObservable } from '@sourcegraph/shared/src/api/client/api/common'
import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { ViewProviderResult } from '@sourcegraph/shared/src/api/extension/extensionHostApi'
import { asError, isErrorLike } from '@sourcegraph/shared/src/util/errors'

import { ViewInsightProviderResult, ViewInsightProviderSourceType } from '../types'

import { getBackendInsights } from './get-backend-insights'

/**
 * Get combined (backend and extensions) code insights unified method.
 * Used for fetching insights in different places (insight, home, directory pages)
 * */
export const getCombinedViews = (
    getExtensionsInsights: () => Observable<ViewProviderResult[]>,
    insightIds?: string[]
): Observable<ViewInsightProviderResult[]> =>
    combineLatest([
        getExtensionsInsights().pipe(
            map(extensionInsights =>
                extensionInsights.map(insight => ({
                    ...insight,
                    // According to our naming convention of insight
                    // <type>.<name>.<render view = insight page | directory | home page>
                    // You can see insight id generation at extension codebase like here
                    // https://github.com/sourcegraph/sourcegraph-search-insights/blob/master/src/search-insights.ts#L86
                    id: insight.id.split('.').slice(0, -1).join('.'),
                    // Convert error like errors since Firefox and Safari don't support
                    // receiving native errors from web worker thread
                    view: isErrorLike(insight.view) ? asError(insight.view) : insight.view,
                    source: ViewInsightProviderSourceType.Extension,
                }))
            )
        ),
        getBackendInsights(insightIds),
    ]).pipe(map(([extensionViews, backendInsights]) => [...backendInsights, ...extensionViews]))

/**
 * Get insights views for the insights page.
 */
export const getInsightCombinedViews = (
    extensionApi: Promise<Remote<FlatExtensionHostAPI>>,
    allInsightIds?: string[],
    backendInsightIds?: string[]
): Observable<ViewInsightProviderResult[]> =>
    getCombinedViews(
        () =>
            from(extensionApi).pipe(
                switchMap(extensionHostAPI =>
                    wrapRemoteObservable(extensionHostAPI.getInsightsViews({}, allInsightIds))
                )
            ),
        backendInsightIds
    )
