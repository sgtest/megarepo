import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { dataOrThrowErrors, gql } from '@sourcegraph/shared/src/graphql/graphql'

import { requestGraphQL } from '../../../../backend/graphql'
import {
    InsightFields,
    InsightsResult,
    SubjectSettingsResult,
    SubjectSettingsVariables,
} from '../../../../graphql-operations'

const insightFieldsFragment = gql`
    fragment InsightFields on Insight {
        title
        description
        series {
            label
            points {
                dateTime
                value
            }
        }
    }
`
export function fetchBackendInsights(): Observable<InsightFields[]> {
    return requestGraphQL<InsightsResult>(gql`
        query Insights {
            insights {
                nodes {
                    ...InsightFields
                }
            }
        }
        ${insightFieldsFragment}
    `).pipe(
        map(dataOrThrowErrors),
        map(data => data.insights?.nodes ?? [])
    )
}

export function fetchLatestSubjectSettings(id: string): Observable<SubjectSettingsResult> {
    return requestGraphQL<SubjectSettingsResult, SubjectSettingsVariables>(
        gql`
            query SubjectSettings($id: ID!) {
                settingsSubject(id: $id) {
                    latestSettings {
                        id
                        contents
                    }
                }
            }
        `,
        { id }
    ).pipe(map(dataOrThrowErrors))
}
