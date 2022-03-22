import { useEffect, useState } from 'react'

import { gql, useApolloClient } from '@apollo/client'

import { IsCodeInsightsLicensedResult } from '../../../graphql-operations'
import { CodeInsightsBackend } from '../core/backend/code-insights-backend'
import { CodeInsightsGqlBackend } from '../core/backend/gql-backend/code-insights-gql-backend'
import { CodeInsightsGqlBackendLimited } from '../core/backend/lam-backend/code-insights-gql-backend-limited'

/**
 * Returns the full or limited version of the API based on
 * whether Code Insights is licensed
 */
export function useGetApi(): CodeInsightsBackend | null {
    const apolloClient = useApolloClient()
    const [api, setApi] = useState<CodeInsightsBackend | null>(null)

    useEffect(() => {
        apolloClient
            .query<IsCodeInsightsLicensedResult>({
                query: gql`
                    query IsCodeInsightsLicensed {
                        enterpriseLicenseHasFeature(feature: "code-insights")
                    }
                `,
            })
            .then(result => {
                const licened = result.data.enterpriseLicenseHasFeature
                setApi(
                    licened ? new CodeInsightsGqlBackend(apolloClient) : new CodeInsightsGqlBackendLimited(apolloClient)
                )
            })
            .catch(() => new Error('Something went wrong fetching the license.'))
    }, [apolloClient])

    return api
}
