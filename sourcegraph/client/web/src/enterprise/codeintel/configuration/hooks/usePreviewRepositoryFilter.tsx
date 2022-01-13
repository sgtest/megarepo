import { ApolloError } from '@apollo/client'

import { gql, useQuery } from '@sourcegraph/http-client'

import { PreviewRepositoryFilterResult, PreviewRepositoryFilterVariables } from '../../../../graphql-operations'

interface SearchRepositoriesResult {
    previewResult: RepositoryPreviewResult | null
    isLoadingPreview: boolean
    previewError: ApolloError | undefined
}

interface RepositoryPreviewResult {
    repositoryNames: string[]
    totalCount: number
    totalMatches: number
    limit: number | null
}

export const PREVIEW_REPOSITORY_FILTER = gql`
    query PreviewRepositoryFilter($patterns: [String!]!) {
        previewRepositoryFilter(patterns: $patterns) {
            nodes {
                name
            }
            totalCount
            totalMatches
            limit
        }
    }
`

export const usePreviewRepositoryFilter = (patterns: string[]): SearchRepositoriesResult => {
    const { data, loading, error } = useQuery<PreviewRepositoryFilterResult, PreviewRepositoryFilterVariables>(
        PREVIEW_REPOSITORY_FILTER,
        {
            variables: {
                patterns,
            },
        }
    )

    return {
        previewResult: data
            ? {
                  ...data.previewRepositoryFilter,
                  repositoryNames: data.previewRepositoryFilter.nodes.map(({ name }) => name),
              }
            : null,
        isLoadingPreview: loading,
        previewError: error,
    }
}
