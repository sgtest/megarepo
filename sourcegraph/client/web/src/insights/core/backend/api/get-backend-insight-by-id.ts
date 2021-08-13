import { Observable, of, throwError } from 'rxjs'
import { map, switchMap } from 'rxjs/operators'

import { fetchBackendInsights } from '../requests/fetch-backend-insights'
import { BackendInsightData, BackendInsightInputs } from '../types'
import { createViewContent } from '../utils/create-view-content'

export function getBackendInsightById(props: BackendInsightInputs): Observable<BackendInsightData> {
    const { id, filters, series } = props

    return fetchBackendInsights([id], filters).pipe(
        switchMap(backendInsights => {
            if (backendInsights.length === 0) {
                return throwError(new Error("We couldn't find insight"))
            }

            return of(backendInsights[0])
        }),
        map(insight => ({
            id: insight.id,
            view: {
                title: insight.title,
                subtitle: insight.description,
                content: [createViewContent(insight, series)],
            },
        }))
    )
}
