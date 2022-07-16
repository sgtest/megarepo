import { FC, useMemo } from 'react'

import {
    SubmissionErrors,
    createDefaultEditSeries,
    CodeInsightsCreationActions,
    CodeInsightCreationMode,
    FORM_ERROR,
} from '../../../../components'
import { MinimalSearchBasedInsightData, SearchBasedInsight } from '../../../../core'
import { CreateInsightFormFields, InsightStep } from '../../creation/search-insight'
import { SearchInsightCreationContent } from '../../creation/search-insight/components/SearchInsightCreationContent'
import { getSanitizedSearchInsight } from '../../creation/search-insight/utils/insight-sanitizer'

interface EditSearchBasedInsightProps {
    licensed: boolean
    isEditAvailable: boolean | undefined
    insight: SearchBasedInsight
    onSubmit: (insight: MinimalSearchBasedInsightData) => SubmissionErrors | Promise<SubmissionErrors> | void
    onCancel: () => void
}

export const EditSearchBasedInsight: FC<EditSearchBasedInsightProps> = props => {
    const { insight, licensed, isEditAvailable, onSubmit, onCancel } = props

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
            seriesDisplayOptions: insight.seriesDisplayOptions,
            seriesCount: insight.seriesCount,
        })
    }

    return (
        <SearchInsightCreationContent
            touched={true}
            initialValue={insightFormValues}
            dataTestId="search-insight-edit-page-content"
            className="pb-5"
            onSubmit={handleSubmit}
        >
            {form => (
                <CodeInsightsCreationActions
                    mode={CodeInsightCreationMode.Edit}
                    licensed={licensed}
                    available={isEditAvailable}
                    submitting={form.submitting}
                    errors={form.submitErrors?.[FORM_ERROR]}
                    clear={form.isFormClearActive}
                    onCancel={onCancel}
                />
            )}
        </SearchInsightCreationContent>
    )
}
