import { FC } from 'react'

import { noop } from 'rxjs'

import {
    CreationUiLayout,
    CreationUIForm,
    CreationUIPreview,
    FormChangeEvent,
    SubmissionErrors,
    createDefaultEditSeries,
    EditableDataSeries,
} from '../../../../../../components'
import { Insight } from '../../../../../../core'
import { LineChartLivePreview, LivePreviewSeries } from '../../../LineChartLivePreview'
import { CreateInsightFormFields } from '../../types'
import { getSanitizedSeries } from '../../utils/insight-sanitizer'
import { SearchInsightCreationForm } from '../SearchInsightCreationForm'

import { useInsightCreationForm } from './hooks/use-insight-creation-form'

export interface SearchInsightCreationContentProps {
    /** This component might be used in edit or creation insight case. */
    mode?: 'creation' | 'edit'

    initialValue?: Partial<CreateInsightFormFields>
    className?: string
    dataTestId?: string
    insight?: Insight

    onSubmit: (values: CreateInsightFormFields) => SubmissionErrors | Promise<SubmissionErrors> | void
    onCancel?: () => void

    /** Change handlers is called every time when user changed any field within the form. */
    onChange?: (event: FormChangeEvent<CreateInsightFormFields>) => void
}

export const SearchInsightCreationContent: FC<SearchInsightCreationContentProps> = props => {
    const {
        mode = 'creation',
        initialValue,
        className,
        dataTestId,
        insight,
        onSubmit,
        onCancel = noop,
        onChange = noop,
    } = props

    const {
        form: { values, formAPI, ref, handleSubmit },
        title,
        repositories,
        series,
        step,
        stepValue,
        allReposMode,
    } = useInsightCreationForm({
        mode,
        initialValue,
        onChange,
        onSubmit,
    })

    const handleFormReset = (): void => {
        // TODO [VK] Change useForm API in order to implement form.reset method.
        title.input.onChange('')
        repositories.input.onChange('')
        // Focus first element of the form
        repositories.input.ref.current?.focus()
        series.input.onChange([createDefaultEditSeries({ edit: true })])
        stepValue.input.onChange('1')
        step.input.onChange('months')
    }

    // If some fields that needed to run live preview  are invalid
    // we should disable live chart preview
    const allFieldsForPreviewAreValid =
        repositories.meta.validState === 'VALID' &&
        (series.meta.validState === 'VALID' || series.input.value.some(series => series.valid)) &&
        stepValue.meta.validState === 'VALID' &&
        // For the "all repositories" mode we are not able to show the live preview chart
        !allReposMode.input.value

    const hasFilledValue =
        values.series?.some(line => line.name !== '' || line.query !== '') ||
        values.repositories !== '' ||
        values.title !== ''

    return (
        <CreationUiLayout data-testid={dataTestId} className={className}>
            <CreationUIForm
                as={SearchInsightCreationForm}
                mode={mode}
                innerRef={ref}
                handleSubmit={handleSubmit}
                submitErrors={formAPI.submitErrors}
                submitting={formAPI.submitting}
                submitted={formAPI.submitted}
                title={title}
                repositories={repositories}
                allReposMode={allReposMode}
                series={series}
                step={step}
                stepValue={stepValue}
                isFormClearActive={hasFilledValue}
                dashboardReferenceCount={initialValue?.dashboardReferenceCount}
                insight={insight}
                onCancel={onCancel}
                onFormReset={handleFormReset}
            />

            <CreationUIPreview
                as={LineChartLivePreview}
                disabled={!allFieldsForPreviewAreValid}
                repositories={repositories.meta.value}
                isAllReposMode={allReposMode.input.value}
                series={seriesToPreview(series.input.value)}
                step={step.meta.value}
                stepValue={stepValue.meta.value}
            />
        </CreationUiLayout>
    )
}

function seriesToPreview(currentSeries: EditableDataSeries[]): LivePreviewSeries[] {
    const validSeries = currentSeries.filter(series => series.valid)
    return getSanitizedSeries(validSeries).map(series => ({
        query: series.query,
        stroke: series.stroke ? series.stroke : '',
        label: series.name,
        generatedFromCaptureGroup: false,
    }))
}
