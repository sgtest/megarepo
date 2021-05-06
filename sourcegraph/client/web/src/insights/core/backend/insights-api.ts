import { Remote } from 'comlink'
import { combineLatest, from, Observable, of } from 'rxjs'
import { catchError, map, switchMap } from 'rxjs/operators'

import { wrapRemoteObservable } from '@sourcegraph/shared/src/api/client/api/common'
import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { ViewProviderResult } from '@sourcegraph/shared/src/api/extension/extensionHostApi'
import { PlatformContext } from '@sourcegraph/shared/src/platform/context'
import { asError } from '@sourcegraph/shared/src/util/errors'

import { fetchBackendInsights, fetchLatestSubjectSettings } from './requests/fetch-backend-insights'
import { ApiService, SubjectSettingsResult, ViewInsightProviderResult, ViewInsightProviderSourceType } from './types'
import { createViewContent } from './utils/create-view-content'

/** Get combined (backend and extensions) code insights */
const getCombinedViews = (
    getExtensionsInsights: () => Observable<ViewProviderResult[]>
): Observable<ViewInsightProviderResult[]> =>
    combineLatest([
        getExtensionsInsights().pipe(
            map(extensionInsights =>
                extensionInsights.map(insight => ({ ...insight, source: ViewInsightProviderSourceType.Extension }))
            )
        ),
        fetchBackendInsights().pipe(
            map(backendInsights =>
                backendInsights.map(
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

const getInsightCombinedViews = (
    extensionApi: Promise<Remote<FlatExtensionHostAPI>>
): Observable<ViewInsightProviderResult[]> =>
    getCombinedViews(() =>
        from(extensionApi).pipe(
            switchMap(extensionHostAPI => wrapRemoteObservable(extensionHostAPI.getInsightsViews({})))
        )
    )

const getSubjectSettings = (id: string): Observable<SubjectSettingsResult> =>
    fetchLatestSubjectSettings(id).pipe(
        map(settings => settings.settingsSubject?.latestSettings ?? { id: null, contents: '' })
    )

const updateSubjectSettings = (
    context: Pick<PlatformContext, 'updateSettings'>,
    subjectId: string,
    content: string
): Observable<void> => from(context.updateSettings(subjectId, content))

/** Main API service to get data for code insights */
export const createInsightAPI = (): ApiService => ({
    getCombinedViews,
    getInsightCombinedViews,
    getSubjectSettings,
    updateSubjectSettings,
})
