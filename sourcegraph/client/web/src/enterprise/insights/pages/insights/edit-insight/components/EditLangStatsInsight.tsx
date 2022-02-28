import React, { useMemo } from 'react'

import { SubmissionErrors } from '../../../../components/form/hooks/useForm'
import { LangStatsInsight } from '../../../../core/types'
import { LangStatsInsightCreationContent } from '../../creation/lang-stats/components/lang-stats-insight-creation-content/LangStatsInsightCreationContent'
import { LangStatsCreationFormFields } from '../../creation/lang-stats/types'
import { getSanitizedLangStatsInsight } from '../../creation/lang-stats/utils/insight-sanitizer'

export interface EditLangStatsInsightProps {
    insight: LangStatsInsight
    onSubmit: (insight: LangStatsInsight) => SubmissionErrors | Promise<SubmissionErrors> | void
    onCancel: () => void
}

export const EditLangStatsInsight: React.FunctionComponent<EditLangStatsInsightProps> = props => {
    const { insight, onSubmit, onCancel } = props

    const insightFormValues = useMemo<LangStatsCreationFormFields>(
        () => ({
            title: insight.title,
            repository: insight.repository,
            threshold: insight.otherThreshold * 100,
            dashboardReferenceCount: insight.dashboardReferenceCount,
        }),
        [insight]
    )

    // Handlers
    const handleSubmit = (values: LangStatsCreationFormFields): SubmissionErrors | Promise<SubmissionErrors> | void => {
        const sanitizedInsight = getSanitizedLangStatsInsight(values)

        return onSubmit(sanitizedInsight)
    }

    return (
        <LangStatsInsightCreationContent
            mode="edit"
            className="pb-5"
            initialValues={insightFormValues}
            onSubmit={handleSubmit}
            onCancel={onCancel}
        />
    )
}
