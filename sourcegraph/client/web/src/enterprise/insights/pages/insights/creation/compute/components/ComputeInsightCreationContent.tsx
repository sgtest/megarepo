import { FC, HTMLAttributes, ReactNode } from 'react'

import { GroupByField } from '@sourcegraph/shared/src/graphql-operations'
import { Code, Input } from '@sourcegraph/wildcard'

import {
    createDefaultEditSeries,
    CreationUIForm,
    CreationUiLayout,
    CreationUIPreview,
    FormChangeEvent,
    FormGroup,
    FormSeries,
    getDefaultInputProps,
    insightRepositoriesAsyncValidator,
    insightRepositoriesValidator,
    insightSeriesValidator,
    insightTitleValidator,
    RepositoriesField,
    SubmissionErrors,
    useField,
    useForm,
} from '../../../../../components'
import { useUiFeatures } from '../../../../../hooks'
import { ComputeLivePreview } from '../../ComputeLivePreview'
import { CreateComputeInsightFormFields } from '../types'

import { ComputeInsightMapPicker } from './ComputeInsightMapPicker'

const INITIAL_INSIGHT_VALUES: CreateComputeInsightFormFields = {
    series: [createDefaultEditSeries({ edit: true })],
    title: '',
    repositories: '',
    groupBy: GroupByField.REPO,
    dashboardReferenceCount: 0,
}

type NativeContainerProps = Omit<HTMLAttributes<HTMLDivElement>, 'onSubmit' | 'onChange' | 'children'>

export interface RenderPropertyInputs {
    submitting: boolean
    submitErrors: SubmissionErrors
    isFormClearActive: boolean
}

interface ComputeInsightCreationContentProps extends NativeContainerProps {
    touched: boolean
    children: (input: RenderPropertyInputs) => ReactNode
    initialValue?: Partial<CreateComputeInsightFormFields>
    onChange?: (event: FormChangeEvent<CreateComputeInsightFormFields>) => void
    onSubmit: (values: CreateComputeInsightFormFields) => SubmissionErrors | Promise<SubmissionErrors> | void
}

export const ComputeInsightCreationContent: FC<ComputeInsightCreationContentProps> = props => {
    const { touched, initialValue, onChange, onSubmit, children, ...attributes } = props
    const { licensed } = useUiFeatures()

    const { formAPI, values, handleSubmit } = useForm<CreateComputeInsightFormFields>({
        initialValues: { ...INITIAL_INSIGHT_VALUES, ...initialValue },
        onSubmit,
        onChange,
        touched,
    })

    const title = useField({
        name: 'title',
        formApi: formAPI,
        validators: { sync: insightTitleValidator },
    })

    const repositories = useField({
        name: 'repositories',
        formApi: formAPI,
        validators: {
            // Turn off any validations for the repositories' field in we are in all repos mode
            sync: insightRepositoriesValidator,
            async: insightRepositoriesAsyncValidator,
        },
    })

    const series = useField({
        name: 'series',
        formApi: formAPI,
        validators: { sync: insightSeriesValidator },
    })

    const groupBy = useField({
        name: 'groupBy',
        formApi: formAPI,
    })

    const handleFormReset = (): void => {
        // TODO [VK] Change useForm API in order to implement form.reset method.
        title.input.onChange('')
        repositories.input.onChange('')
        series.input.onChange([createDefaultEditSeries({ edit: true })])

        // Focus first element of the form
        repositories.input.ref.current?.focus()
    }

    const hasFilledValue =
        values.series?.some(line => line.name !== '' || line.query !== '') ||
        values.repositories !== '' ||
        values.title !== ''

    // If some fields that needed to run live preview  are invalid
    // we should disable live chart preview
    const allFieldsForPreviewAreValid =
        repositories.meta.validState === 'VALID' &&
        (series.meta.validState === 'VALID' || series.meta.value.some(series => series.valid))

    const validSeries = series.meta.value.filter(series => series.valid)

    return (
        <CreationUiLayout {...attributes}>
            <CreationUIForm noValidate={true} onSubmit={handleSubmit} onReset={handleFormReset}>
                <FormGroup
                    name="insight repositories"
                    title="Targeted repositories"
                    subtitle="Create a list of repositories to run your search over"
                >
                    <Input
                        as={RepositoriesField}
                        autoFocus={true}
                        required={true}
                        label="Repositories"
                        message="Separate repositories with commas"
                        placeholder="Example: github.com/sourcegraph/sourcegraph"
                        {...getDefaultInputProps(repositories)}
                        className="mb-0 d-flex flex-column"
                    />
                </FormGroup>

                <hr className="my-4 w-100" />

                <FormGroup
                    innerRef={series.input.ref}
                    name="data series group"
                    title="Data series"
                    error={series.meta.touched && series.meta.error}
                    subtitle={
                        licensed
                            ? 'Add any number of data series to your chart'
                            : 'Add up to 10 data series to your chart'
                    }
                >
                    <FormSeries
                        seriesField={series}
                        repositories={repositories.input.value}
                        showValidationErrorsOnMount={formAPI.submitted}
                        showColorPicker={false}
                        queryFieldDescription={
                            <ul className="pl-3">
                                <li>
                                    Do not include the <Code weight="bold">repo:</Code> filter as it will be added
                                    automatically, if needed{' '}
                                </li>
                                <li>
                                    You can use <Code weight="bold">before:</Code> and <Code weight="bold">after:</Code>{' '}
                                    operators for <Code weight="bold">type:diff</Code> and{' '}
                                    <Code weight="bold">type:commit</Code> to define the timeframe (example query:{' '}
                                    <Code>type:diff author:nick before:"last thursday" SearchTerm</Code>)
                                </li>
                            </ul>
                        }
                    />
                </FormGroup>

                <hr className="my-4 w-100" />

                <FormGroup name="map result" title="Map result">
                    <ComputeInsightMapPicker series={validSeries} {...groupBy.input} />
                </FormGroup>

                <hr className="my-4 w-100" />

                <FormGroup name="chart settings group" title="Chart settings">
                    <Input
                        label="Title"
                        required={true}
                        message="Shown as the title for your insight"
                        placeholder="Example: Migration to React function components"
                        className="d-flex flex-column"
                        {...getDefaultInputProps(title)}
                    />
                </FormGroup>

                <hr className="my-4 w-100" />

                {children({
                    submitting: formAPI.submitting,
                    submitErrors: formAPI.submitErrors,
                    isFormClearActive: hasFilledValue,
                })}
            </CreationUIForm>

            <CreationUIPreview
                as={ComputeLivePreview}
                disabled={!allFieldsForPreviewAreValid}
                repositories={repositories.meta.value}
                series={validSeries}
                groupBy={groupBy.meta.value}
            />
        </CreationUiLayout>
    )
}
