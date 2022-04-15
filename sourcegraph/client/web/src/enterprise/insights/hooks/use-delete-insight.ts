import { useCallback, useContext, useState } from 'react'

import { ErrorLike } from '@sourcegraph/common'

import { eventLogger } from '../../../tracking/eventLogger'
import { CodeInsightsBackendContext, Insight } from '../core'
import { getTrackingTypeByInsightType } from '../pings'

type DeletionInsight = Pick<Insight, 'id' | 'title' | 'type'>

export interface UseDeleteInsightAPI {
    delete: (insight: DeletionInsight) => Promise<void>
    loading: boolean
    error: ErrorLike | undefined
}

/**
 * Returns delete handler that deletes insight from all subject settings and from all dashboards
 * that include this insight.
 */
export function useDeleteInsight(): UseDeleteInsightAPI {
    const { deleteInsight } = useContext(CodeInsightsBackendContext)

    const [loading, setLoading] = useState<boolean>(false)
    const [error, setError] = useState<ErrorLike | undefined>()

    const handleDelete = useCallback(
        async (insight: DeletionInsight) => {
            const shouldDelete = window.confirm(`Are you sure you want to delete the insight "${insight.title}"?`)

            // Prevent double call if we already have ongoing request
            if (loading || !shouldDelete) {
                return
            }

            setLoading(true)
            setError(undefined)

            try {
                await deleteInsight(insight.id).toPromise()
                const insightType = getTrackingTypeByInsightType(insight.type)

                eventLogger.log('InsightRemoval', { insightType }, { insightType })
            } catch (error) {
                // TODO [VK] Improve error UI for deleting
                console.error(error)
                setError(error)
            }
        },
        [loading, deleteInsight]
    )

    return { delete: handleDelete, loading, error }
}
