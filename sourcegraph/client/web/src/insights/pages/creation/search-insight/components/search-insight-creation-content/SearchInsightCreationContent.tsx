import classnames from 'classnames'
import React from 'react'
import { noop } from 'rxjs'

import { Settings } from '@sourcegraph/shared/src/settings/settings'

import { useField } from '../../../../../components/form/hooks/useField'
import { SubmissionErrors, useForm } from '../../../../../components/form/hooks/useForm'
import { useTitleValidator } from '../../../../../components/form/hooks/useTitleValidator'
import { Organization } from '../../../../../components/visibility-picker/VisibilityPicker'
import { InsightTypePrefix } from '../../../../../core/types'
import { CreateInsightFormFields } from '../../types'
import { SearchInsightLivePreview } from '../live-preview-chart/SearchInsightLivePreview'
import { SearchInsightCreationForm } from '../search-insight-creation-form/SearchInsightCreationForm'

import { useEditableSeries } from './hooks/use-editable-series'
import styles from './SearchInsightCreationContent.module.scss'
import {
    repositoriesExistValidator,
    repositoriesFieldValidator,
    requiredStepValueField,
    seriesRequired,
} from './validators'

const INITIAL_VALUES: CreateInsightFormFields = {
    visibility: 'personal',
    series: [],
    step: 'months',
    stepValue: '2',
    title: '',
    repositories: '',
}

export interface SearchInsightCreationContentProps {
    /** This component might be used in edit or creation insight case. */
    mode?: 'creation' | 'edit'
    /** Final settings cascade. Used for title field validation. */
    settings?: Settings | null
    /** List of all user organizations */
    organizations?: Organization[]
    /** Initial value for all form fields. */
    initialValue?: CreateInsightFormFields
    /** Custom class name for root form element. */
    className?: string
    /** Submit handler for form element. */
    onSubmit: (values: CreateInsightFormFields) => SubmissionErrors | Promise<SubmissionErrors> | void
    /** Cancel handler. */
    onCancel?: () => void
}

export const SearchInsightCreationContent: React.FunctionComponent<SearchInsightCreationContentProps> = props => {
    const {
        mode = 'creation',
        organizations = [],
        settings,
        initialValue = INITIAL_VALUES,
        onSubmit,
        onCancel = noop,
        className,
    } = props

    const isEditMode = mode === 'edit'

    const { formAPI, ref, handleSubmit } = useForm<CreateInsightFormFields>({
        initialValues: initialValue,
        onSubmit,
        touched: isEditMode,
    })

    // We can't have two or more insights with the same name, since we rely on name as on id of insights.
    const titleValidator = useTitleValidator({ settings, insightType: InsightTypePrefix.search })

    const title = useField('title', formAPI, { sync: titleValidator })
    const repositories = useField('repositories', formAPI, {
        sync: repositoriesFieldValidator,
        async: repositoriesExistValidator,
    })
    const visibility = useField('visibility', formAPI)

    const series = useField('series', formAPI, { sync: seriesRequired })
    const step = useField('step', formAPI)
    const stepValue = useField('stepValue', formAPI, { sync: requiredStepValueField })

    const { liveSeries, editSeries, listen, editRequest, editCommit, cancelEdit, deleteSeries } = useEditableSeries({
        series,
    })

    // If some fields that needed to run live preview  are invalid
    // we should disabled live chart preview
    const allFieldsForPreviewAreValid =
        (repositories.meta.validState === 'VALID' || repositories.meta.validState === 'CHECKING') &&
        (series.meta.validState === 'VALID' || liveSeries.length) &&
        stepValue.meta.validState === 'VALID'

    return (
        <div className={classnames(styles.content, className)}>
            <SearchInsightCreationForm
                mode={mode}
                className={styles.contentForm}
                innerRef={ref}
                handleSubmit={handleSubmit}
                submitErrors={formAPI.submitErrors}
                submitting={formAPI.submitting}
                title={title}
                repositories={repositories}
                visibility={visibility}
                organizations={organizations}
                series={series}
                step={step}
                stepValue={stepValue}
                onSeriesLiveChange={listen}
                onCancel={onCancel}
                editSeries={editSeries}
                onEditSeriesRequest={editRequest}
                onEditSeriesCancel={cancelEdit}
                onEditSeriesCommit={editCommit}
                onSeriesRemove={deleteSeries}
            />

            <SearchInsightLivePreview
                disabled={!allFieldsForPreviewAreValid}
                repositories={repositories.meta.value}
                series={liveSeries}
                step={step.meta.value}
                stepValue={stepValue.meta.value}
                className={styles.contentLivePreview}
            />
        </div>
    )
}
