import { useContext } from 'react'

import { useNavigate } from 'react-router-dom'
import { lastValueFrom } from 'rxjs'

import { TelemetryV2Props } from '@sourcegraph/shared/src/telemetry'
import { EVENT_LOGGER } from '@sourcegraph/shared/src/telemetry/web/eventLogger'
import type { SubmissionErrors } from '@sourcegraph/wildcard'

import { CodeInsightsBackendContext, type CreationInsightInput } from '../../../../core'
import { useQueryParameters } from '../../../../hooks'
import { getTrackingTypeByInsightType } from '../../../../pings'
import { V2InsightType } from '../../../../pings/types'

export interface useHandleSubmitOutput {
    handleSubmit: (newInsight: CreationInsightInput) => Promise<SubmissionErrors>
    handleCancel: () => void
}

interface Props extends TelemetryV2Props {
    id: string | undefined
}

/**
 * Returns submit and cancel handlers for the insight edit submit page.
 */
export function useEditPageHandlers(props: Props): useHandleSubmitOutput {
    const { id, telemetryRecorder } = props
    const { updateInsight } = useContext(CodeInsightsBackendContext)
    const navigate = useNavigate()

    const { dashboardId, insight } = useQueryParameters(['dashboardId', 'insight'])
    const redirectUrl = getReturnToLink(insight, dashboardId)

    const handleSubmit = async (newInsight: CreationInsightInput): Promise<SubmissionErrors> => {
        if (!id) {
            return
        }

        await lastValueFrom(
            updateInsight({
                insightId: id,
                nextInsightData: newInsight,
            }),
            { defaultValue: undefined }
        )

        const insightType = getTrackingTypeByInsightType(newInsight.type)
        EVENT_LOGGER.log('InsightEdit', { insightType }, { insightType })
        telemetryRecorder.recordEvent('insight', 'edit', { metadata: { type: V2InsightType[insightType] } })
        navigate(redirectUrl)
    }

    const handleCancel = (): void => {
        navigate(redirectUrl)
    }

    return { handleSubmit, handleCancel }
}

function getReturnToLink(insightId: string | undefined, dashboardId: string | undefined): string {
    if (insightId) {
        return `/insights/insight/${insightId}`
    }

    if (dashboardId) {
        return `/insights/dashboards/${dashboardId}`
    }

    return '/insights/all'
}
