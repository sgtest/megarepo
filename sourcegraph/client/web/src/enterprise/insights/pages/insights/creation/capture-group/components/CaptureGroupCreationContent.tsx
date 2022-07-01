import { FC, useCallback } from 'react'

import { noop } from 'lodash'

import {
    CreationUiLayout,
    CreationUIForm,
    CreationUIPreview,
    useAsyncInsightTitleValidator,
    useField,
    FormChangeEvent,
    SubmissionErrors,
    useForm,
    createRequiredValidator,
} from '../../../../../components'
import { Insight } from '../../../../../core'
import { LineChartLivePreview } from '../../LineChartLivePreview'
import {
    repositoriesExistValidator,
    repositoriesFieldValidator,
    requiredStepValueField,
} from '../../search-insight/components/search-insight-creation-content/validators'
import { CaptureGroupFormFields } from '../types'
import { searchQueryValidator } from '../utils/search-query-validator'

import { CaptureGroupCreationForm } from './CaptureGoupCreationForm'

const INITIAL_VALUES: CaptureGroupFormFields = {
    repositories: '',
    groupSearchQuery: '',
    title: '',
    step: 'months',
    stepValue: '2',
    allRepos: false,
    dashboardReferenceCount: 0,
}

const titleRequiredValidator = createRequiredValidator('Title is a required field.')
const queryRequiredValidator = createRequiredValidator('Query is a required field.')

interface CaptureGroupCreationContentProps {
    mode: 'creation' | 'edit'
    initialValues?: Partial<CaptureGroupFormFields>
    className?: string
    insight?: Insight

    onSubmit: (values: CaptureGroupFormFields) => SubmissionErrors | Promise<SubmissionErrors> | void
    onChange?: (event: FormChangeEvent<CaptureGroupFormFields>) => void
    onCancel: () => void
}

export const CaptureGroupCreationContent: FC<CaptureGroupCreationContentProps> = props => {
    const { mode, className, initialValues = {}, onSubmit, onChange = noop, onCancel, insight } = props

    // Search query validators
    const validateChecks = useCallback((value: string | undefined) => {
        if (!value) {
            return queryRequiredValidator(value)
        }

        const validatedChecks = searchQueryValidator(value, true)
        const allChecksPassed = Object.values(validatedChecks).every(Boolean)

        if (!allChecksPassed) {
            return 'Query is not valid'
        }

        return queryRequiredValidator(value)
    }, [])

    const form = useForm<CaptureGroupFormFields>({
        initialValues: { ...INITIAL_VALUES, ...initialValues },
        touched: mode === 'edit',
        onSubmit,
        onChange,
    })

    const asyncTitleValidator = useAsyncInsightTitleValidator({
        mode,
        initialTitle: form.formAPI.initialValues.title,
    })

    const title = useField({
        name: 'title',
        formApi: form.formAPI,
        validators: { sync: titleRequiredValidator, async: asyncTitleValidator },
    })

    const allReposMode = useField({
        name: 'allRepos',
        formApi: form.formAPI,
        onChange: (checked: boolean) => {
            // Reset form values in case if All repos mode was activated
            if (checked) {
                repositories.input.onChange('')
                step.input.onChange('months')
                stepValue.input.onChange('1')
            }
        },
    })

    const isAllReposMode = allReposMode.input.value

    const repositories = useField({
        name: 'repositories',
        formApi: form.formAPI,
        validators: {
            // Turn off any validations for the repositories field in we are in all repos mode
            sync: !isAllReposMode ? repositoriesFieldValidator : undefined,
            async: !isAllReposMode ? repositoriesExistValidator : undefined,
        },
        disabled: isAllReposMode,
    })

    const query = useField({
        name: 'groupSearchQuery',
        formApi: form.formAPI,
        validators: { sync: validateChecks },
    })

    const step = useField({
        name: 'step',
        formApi: form.formAPI,
    })

    const stepValue = useField({
        name: 'stepValue',
        formApi: form.formAPI,
        validators: {
            sync: requiredStepValueField,
        },
    })

    const handleFormReset = (): void => {
        title.input.onChange('')
        repositories.input.onChange('')
        query.input.onChange('')
        step.input.onChange('months')
        stepValue.input.onChange('1')

        // Focus first element of the form
        repositories.input.ref.current?.focus()
    }

    const hasFilledValue =
        form.values.title !== '' || form.values.repositories !== '' || form.values.groupSearchQuery !== ''

    const areAllFieldsForPreviewValid =
        repositories.meta.validState === 'VALID' &&
        stepValue.meta.validState === 'VALID' &&
        query.meta.validState === 'VALID' &&
        // For all repos mode we are not able to show the live preview chart
        !allReposMode.input.value

    return (
        <CreationUiLayout className={className}>
            <CreationUIForm
                as={CaptureGroupCreationForm}
                mode={mode}
                form={form}
                title={title}
                repositories={repositories}
                step={step}
                stepValue={stepValue}
                query={query}
                isFormClearActive={hasFilledValue}
                allReposMode={allReposMode}
                dashboardReferenceCount={initialValues.dashboardReferenceCount}
                insight={insight}
                onCancel={onCancel}
                onFormReset={handleFormReset}
            />

            <CreationUIPreview
                as={LineChartLivePreview}
                disabled={!areAllFieldsForPreviewValid}
                isAllReposMode={allReposMode.input.value}
                repositories={repositories.meta.value}
                series={captureGroupPreviewSeries(query.meta.value)}
                step={step.meta.value}
                stepValue={stepValue.meta.value}
            />
        </CreationUiLayout>
    )
}

function captureGroupPreviewSeries(query: string): any {
    return [
        {
            generatedFromCaptureGroup: true,
            label: '',
            query,
            stroke: '',
        },
    ]
}
