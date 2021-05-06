import classnames from 'classnames'
import { camelCase } from 'lodash'
import React, { useMemo } from 'react'
import { noop } from 'rxjs'

import { Settings } from '@sourcegraph/shared/src/settings/settings'

import { ErrorAlert } from '../../../../../components/alerts'
import { LoaderButton } from '../../../../../components/LoaderButton'
import { InsightTypeSuffix } from '../../../../core/types'
import { useField, Validator } from '../../hooks/useField'
import { FORM_ERROR, SubmissionErrors, useForm } from '../../hooks/useForm'
import { DataSeries } from '../../types'
import { FormGroup } from '../form-group/FormGroup'
import { InputField } from '../form-input-field/InputField'
import { FormRadioInput } from '../form-radio-input/FormRadioInput'
import { FormSeries } from '../form-series/FormSeries'
import { createRequiredValidator, composeValidators } from '../validators'

import styles from './CreationSearchInsightForm.module.scss'

const repositoriesFieldValidator = createRequiredValidator('Repositories is a required field.')
const requiredStepValueField = createRequiredValidator('Please specify a step between points.')
/**
 * Custom validator for chart series. Since series has complex type
 * we can't validate this with standard validators.
 * */
const seriesRequired: Validator<DataSeries[]> = series =>
    series && series.length > 0 ? undefined : 'Series is empty. You must have at least one series for code insight.'

const INITIAL_VALUES: Partial<CreateInsightFormFields> = {
    visibility: 'personal',
    series: [],
    step: 'months',
    stepValue: '2',
    title: '',
    repositories: '',
}

/** Default value for final user/org settings cascade */
const DEFAULT_FINAL_SETTINGS = {}

/** Public API of code insight creation form. */
export interface CreationSearchInsightFormProps {
    /** Final settings cascade. Used for title field validation. */
    settings?: Settings | null
    /** Custom class name for root form element. */
    className?: string
    /** Submit handler for form element. */
    onSubmit: (values: CreateInsightFormFields) => SubmissionErrors | Promise<SubmissionErrors> | void
    onCancel?: () => void
}

/** Creation form fields. */
export interface CreateInsightFormFields {
    /** Code Insight series setting (name of line, line query, color) */
    series: DataSeries[]
    /** Title of code insight*/
    title: string
    /** Repositories which to be used to get the info for code insights */
    repositories: string
    /** Visibility setting which responsible for where insight will appear. */
    visibility: 'personal' | 'organization'
    /** Setting for set chart step - how often do we collect data. */
    step: 'hours' | 'days' | 'weeks' | 'months' | 'years'
    /** Value for insight step setting */
    stepValue: string
}

/** Displays creation code insight form (title, visibility, series, etc.) */
export const CreationSearchInsightForm: React.FunctionComponent<CreationSearchInsightFormProps> = props => {
    const { settings, className, onSubmit, onCancel = noop } = props

    const { formAPI, ref, handleSubmit } = useForm<CreateInsightFormFields>({
        initialValues: INITIAL_VALUES,
        onSubmit,
    })

    // We can't have two or more insights with the same name, since we rely on name as on id of insights.
    const titleValidator = useMemo(() => {
        const alreadyExistsInsightNames = new Set(
            Object.keys(settings ?? DEFAULT_FINAL_SETTINGS)
                // According to our convention about insights name <insight type>.insight.<insight name>
                .filter(key => key.startsWith(InsightTypeSuffix.search))
                .map(key => camelCase(key.split('.').pop()))
        )

        return composeValidators<string>(createRequiredValidator('Title is a required field.'), value =>
            alreadyExistsInsightNames.has(camelCase(value))
                ? 'An insight with this name already exists. Please set a different name for the new insight.'
                : undefined
        )
    }, [settings])

    const title = useField('title', formAPI, titleValidator)
    const repositories = useField('repositories', formAPI, repositoriesFieldValidator)
    const visibility = useField('visibility', formAPI)

    const series = useField('series', formAPI, seriesRequired)
    const step = useField('step', formAPI)
    const stepValue = useField('stepValue', formAPI, requiredStepValueField)

    return (
        // eslint-disable-next-line react/forbid-elements
        <form
            noValidate={true}
            ref={ref}
            onSubmit={handleSubmit}
            className={classnames(className, 'd-flex flex-column')}
        >
            <InputField
                title="Title"
                autoFocus={true}
                required={true}
                description="Shown as the title for your insight"
                placeholder="ex. Migration to React function components"
                valid={title.meta.touched && title.meta.validState === 'VALID'}
                error={title.meta.touched && title.meta.error}
                {...title.input}
                className="mb-0"
            />

            <InputField
                title="Repositories"
                required={true}
                description="Create a list of repositories to run your search over. Separate them with commas."
                placeholder="Add or search for repositories"
                valid={repositories.meta.touched && repositories.meta.validState === 'VALID'}
                error={repositories.meta.touched && repositories.meta.error}
                {...repositories.input}
                className="mb-0 mt-4"
            />

            <FormGroup
                name="visibility"
                title="Visibility"
                description="This insight will be visible only on your personal dashboard. It will not be show to other
                            users in your organization."
                className="mb-0 mt-4"
                contentClassName="d-flex flex-wrap mb-n2"
            >
                <FormRadioInput
                    name="visibility"
                    value="personal"
                    title="Personal"
                    description="only you"
                    checked={visibility.input.value === 'personal'}
                    className="mr-3"
                    onChange={visibility.input.onChange}
                />

                <FormRadioInput
                    name="visibility"
                    value="organization"
                    title="Organization"
                    description="all users in your organization"
                    checked={visibility.input.value === 'organization'}
                    onChange={visibility.input.onChange}
                    className="mr-3"
                />
            </FormGroup>

            <hr className={styles.creationInsightFormSeparator} />

            <FormGroup
                name="data series group"
                title="Data series"
                subtitle="Add any number of data series to your chart"
                error={series.meta.touched && series.meta.error}
                innerRef={series.input.ref}
                className="mb-0"
            >
                <FormSeries series={series.input.value} onChange={series.input.onChange} />
            </FormGroup>

            <hr className={styles.creationInsightFormSeparator} />

            <FormGroup
                name="insight step group"
                title="Step between data points"
                description="The distance between two data points on the chart"
                error={stepValue.meta.touched && stepValue.meta.error}
                className="mb-0"
                contentClassName="d-flex flex-wrap mb-n2"
            >
                <InputField
                    placeholder="ex. 2"
                    required={true}
                    type="number"
                    min={1}
                    {...stepValue.input}
                    valid={stepValue.meta.touched && stepValue.meta.validState === 'VALID'}
                    errorInputState={stepValue.meta.touched && stepValue.meta.validState === 'INVALID'}
                    className={classnames(styles.creationInsightFormStepInput)}
                />

                <FormRadioInput
                    title="Hours"
                    name="step"
                    value="hours"
                    checked={step.input.value === 'hours'}
                    onChange={step.input.onChange}
                    className="mr-3"
                />
                <FormRadioInput
                    title="Days"
                    name="step"
                    value="days"
                    checked={step.input.value === 'days'}
                    onChange={step.input.onChange}
                    className="mr-3"
                />
                <FormRadioInput
                    title="Weeks"
                    name="step"
                    value="weeks"
                    checked={step.input.value === 'weeks'}
                    onChange={step.input.onChange}
                    className="mr-3"
                />
                <FormRadioInput
                    title="Months"
                    name="step"
                    value="months"
                    checked={step.input.value === 'months'}
                    onChange={step.input.onChange}
                    className="mr-3"
                />
                <FormRadioInput
                    title="Years"
                    name="step"
                    value="years"
                    checked={step.input.value === 'years'}
                    onChange={step.input.onChange}
                    className="mr-3"
                />
            </FormGroup>

            <hr className={styles.creationInsightFormSeparator} />

            <div>
                {formAPI.submitErrors?.[FORM_ERROR] && <ErrorAlert error={formAPI.submitErrors[FORM_ERROR]} />}

                <LoaderButton
                    alwaysShowLabel={true}
                    loading={formAPI.submitting}
                    label={formAPI.submitting ? 'Submitting' : 'Create code insight'}
                    type="submit"
                    disabled={formAPI.submitting}
                    className="btn btn-primary mr-2"
                />

                <button type="button" className="btn btn-outline-secondary" onClick={onCancel}>
                    Cancel
                </button>
            </div>
        </form>
    )
}
