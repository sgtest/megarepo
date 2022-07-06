import React, { useMemo } from 'react'

import { SubmissionErrors, createDefaultEditSeries } from '../../../../components'
import { MinimalSearchBasedInsightData, SearchBasedInsight } from '../../../../core'
import { CreateInsightFormFields, InsightStep } from '../../creation/search-insight'
import { SearchInsightCreationContent } from '../../creation/search-insight/components/search-insight-creation-content/SearchInsightCreationContent'
import { getSanitizedSearchInsight } from '../../creation/search-insight/utils/insight-sanitizer'

interface EditSearchBasedInsightProps {
    insight: SearchBasedInsight
    onSubmit: (insight: MinimalSearchBasedInsightData) => SubmissionErrors | Promise<SubmissionErrors> | void
    onCancel: () => void
}

export const EditSearchBasedInsight: React.FunctionComponent<
    React.PropsWithChildren<EditSearchBasedInsightProps>
> = props => {
    const { insight, onSubmit, onCancel } = props

    const insightFormValues = useMemo<CreateInsightFormFields>(
        () => ({
            title: insight.title,
            repositories: insight.repositories.join(', '),
            series: insight.series.map(line => createDefaultEditSeries({ ...line, valid: true })),
            stepValue: Object.values(insight.step)[0]?.toString() ?? '3',
            step: Object.keys(insight.step)[0] as InsightStep,
            allRepos: insight.repositories.length === 0,
            dashboardReferenceCount: insight.dashboardReferenceCount,
        }),
        [insight]
    )

    const handleSubmit = (values: CreateInsightFormFields): SubmissionErrors | Promise<SubmissionErrors> | void => {
        const sanitizedInsight = getSanitizedSearchInsight(values)
        return onSubmit({
            ...sanitizedInsight,
            filters: insight.filters,
        })
    }

    return (
        <SearchInsightCreationContent
            mode="edit"
            initialValue={insightFormValues}
            dataTestId="search-insight-edit-page-content"
            className="pb-5"
            onSubmit={handleSubmit}
            onCancel={onCancel}
            insight={insight}
        />
    )
}
