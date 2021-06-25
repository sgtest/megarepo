import { Remote } from 'comlink'
import { combineLatest, from, Observable, of } from 'rxjs'
import { catchError, map, startWith, switchMap } from 'rxjs/operators'

import { wrapRemoteObservable } from '@sourcegraph/shared/src/api/client/api/common'
import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { ViewProviderResult } from '@sourcegraph/shared/src/api/extension/extensionHostApi'
import { asError, isErrorLike } from '@sourcegraph/shared/src/util/errors'

import { fetchBackendInsights } from '../requests/fetch-backend-insights'
import { ViewInsightProviderResult, ViewInsightProviderSourceType } from '../types'
import { createViewContent } from '../utils/create-view-content'

/**
 * Get combined (backend and extensions) code insights unified method.
 * Used for fetching insights in different places (insight, home, directory pages)
 * */
export const getCombinedViews = (
    getExtensionsInsights: () => Observable<ViewProviderResult[]>
): Observable<ViewInsightProviderResult[]> =>
    combineLatest([
        getExtensionsInsights().pipe(
            map(extensionInsights =>
                extensionInsights.map(insight => ({
                    ...insight,
                    // Convert error like errors since Firefox and Safari don't support
                    // receiving native errors from web worker thread
                    view: isErrorLike(insight.view) ? asError(insight.view) : insight.view,
                    source: ViewInsightProviderSourceType.Extension,
                }))
            )
        ),
        fetchBackendInsights().pipe(
            startWith(null),
            map(backendInsights =>
                backendInsights === null
                    ? [{ id: 'Backend insights', view: undefined, source: ViewInsightProviderSourceType.Backend }]
                    : backendInsights?.map(
                          (insight, index): ViewInsightProviderResult => ({
                              id: `Backend insight ${index + 1}`,
                              view: {
                                  title: insight.title,
                                  subtitle: insight.description,
                                  content: [createViewContent(insight)],
                              },
                              source: ViewInsightProviderSourceType.Backend,
                          })
                      )
            ),
            catchError(error =>
                of<ViewInsightProviderResult[]>([
                    {
                        id: 'Backend insight',
                        view: asError(error),
                        source: ViewInsightProviderSourceType.Backend,
                    },
                ])
            )
        ),
    ]).pipe(map(([extensionViews, backendInsights]) => [...backendInsights, ...extensionViews]))

/**
 * Get insights views for the insights page.
 */
export const getInsightCombinedViews = (
    extensionApi: Promise<Remote<FlatExtensionHostAPI>>,
    insightIds?: string[]
): Observable<ViewInsightProviderResult[]> =>
    getCombinedViews(() =>
        from(extensionApi).pipe(
            switchMap(extensionHostAPI => wrapRemoteObservable(extensionHostAPI.getInsightsViews({}, insightIds)))
        )
    )
