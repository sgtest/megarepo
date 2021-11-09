import classNames from 'classnames'
import React, { FormEventHandler, RefObject, useContext } from 'react'

import { Button } from '@sourcegraph/wildcard'

import { ErrorAlert } from '../../../../../../../../components/alerts'
import { LoaderButton } from '../../../../../../../../components/LoaderButton'
import { FormGroup } from '../../../../../../components/form/form-group/FormGroup'
import { FormInput } from '../../../../../../components/form/form-input/FormInput'
import { FormRadioInput } from '../../../../../../components/form/form-radio-input/FormRadioInput'
import { useFieldAPI } from '../../../../../../components/form/hooks/useField'
import { FORM_ERROR, SubmissionErrors } from '../../../../../../components/form/hooks/useForm'
import { RepositoriesField } from '../../../../../../components/form/repositories-field/RepositoriesField'
import { VisibilityPicker } from '../../../../../../components/visibility-picker/VisibilityPicker'
import { CodeInsightsBackendContext } from '../../../../../../core/backend/code-insights-backend-context'
import { CodeInsightsGqlBackend } from '../../../../../../core/backend/code-insights-gql-backend'
import { SupportedInsightSubject } from '../../../../../../core/types/subjects'
import { CreateInsightFormFields, EditableDataSeries } from '../../types'
import { FormSeries } from '../form-series/FormSeries'

import styles from './SearchInsightCreationForm.module.scss'

interface CreationSearchInsightFormProps {
    /** This component might be used in edit or creation insight case. */
    mode?: 'creation' | 'edit'

    innerRef: RefObject<any>
    handleSubmit: FormEventHandler
    submitErrors: SubmissionErrors
    submitting: boolean
    submitted: boolean
    className?: string
    isFormClearActive?: boolean

    title: useFieldAPI<CreateInsightFormFields['title']>
    repositories: useFieldAPI<CreateInsightFormFields['repositories']>
    allReposMode: useFieldAPI<CreateInsightFormFields['allRepos']>

    visibility: useFieldAPI<CreateInsightFormFields['visibility']>
    subjects: SupportedInsightSubject[]

    series: useFieldAPI<CreateInsightFormFields['series']>
    step: useFieldAPI<CreateInsightFormFields['step']>
    stepValue: useFieldAPI<CreateInsightFormFields['stepValue']>

    onCancel: () => void

    /**
     * Handler to listen latest value form particular series edit form
     * Used to get information for live preview chart.
     */
    onSeriesLiveChange: (liveSeries: EditableDataSeries, isValid: boolean, index: number) => void

    /**
     * Handlers for CRUD operation over series. Add, delete, update and cancel
     * series edit form.
     */
    onEditSeriesRequest: (openedCardIndex: number) => void
    onEditSeriesCommit: (seriesIndex: number, editedSeries: EditableDataSeries) => void
    onEditSeriesCancel: (closedCardIndex: number) => void
    onSeriesRemove: (removedSeriesIndex: number) => void

    onFormReset: () => void
}

/**
 * Displays creation code insight form (title, visibility, series, etc.)
 * UI layer only, all controlled data should be managed by consumer of this component.
 */
export const SearchInsightCreationForm: React.FunctionComponent<CreationSearchInsightFormProps> = props => {
    const {
        mode,
        innerRef,
        handleSubmit,
        submitErrors,
        submitting,
        submitted,
        title,
        repositories,
        allReposMode,
        visibility,
        subjects,
        series,
        stepValue,
        step,
        className,
        isFormClearActive,
        onCancel,
        onSeriesLiveChange,
        onEditSeriesRequest,
        onEditSeriesCommit,
        onEditSeriesCancel,
        onSeriesRemove,
        onFormReset,
    } = props

    const isEditMode = mode === 'edit'

    const api = useContext(CodeInsightsBackendContext)

    // We have to know about what exactly api we use to be able switch our UI properly.
    // In the creation UI case we should hide visibility section since we don't use that
    // concept anymore with new GQL backend.
    // TODO [VK]: Remove this condition rendering when we deprecate setting-based api
    const isGqlBackend = api instanceof CodeInsightsGqlBackend

    return (
        // eslint-disable-next-line react/forbid-elements
        <form
            noValidate={true}
            ref={innerRef}
            onSubmit={handleSubmit}
            onReset={onFormReset}
            className={classNames(className, 'd-flex flex-column')}
        >
            <FormGroup
                name="insight repositories"
                title="Targeted repositories"
                subtitle="Create a list of repositories to run your search over"
            >
                <FormInput
                    as={RepositoriesField}
                    autoFocus={true}
                    required={true}
                    title="Repositories"
                    description="Separate repositories with commas"
                    placeholder={
                        allReposMode.input.value ? 'All repositories' : 'Example: github.com/sourcegraph/sourcegraph'
                    }
                    loading={repositories.meta.validState === 'CHECKING'}
                    valid={repositories.meta.touched && repositories.meta.validState === 'VALID'}
                    error={repositories.meta.touched && repositories.meta.error}
                    {...repositories.input}
                    className="mb-0 d-flex flex-column"
                />

                <label className="d-flex flex-wrap align-items-center mb-2 mt-3 font-weight-normal">
                    <input
                        type="checkbox"
                        {...allReposMode.input}
                        value="all-repos-mode"
                        checked={allReposMode.input.value}
                    />

                    <span className="pl-2">Run your insight over all your repositories</span>

                    <small className="w-100 mt-2 text-muted">
                        This feature is actively in development. Read about the{' '}
                        <a
                            href="https://docs.sourcegraph.com/code_insights/explanations/current_limitations_of_code_insights"
                            target="_blank"
                            rel="noopener noreferrer"
                        >
                            beta limitations here.
                        </a>
                    </small>
                </label>

                <hr className={styles.creationInsightFormSeparator} />
            </FormGroup>

            <FormGroup
                name="data series group"
                title="Data series"
                subtitle="Add any number of data series to your chart"
                error={series.meta.touched && series.meta.error}
                innerRef={series.input.ref}
            >
                <FormSeries
                    series={series.input.value}
                    isBackendInsightEdit={isEditMode && allReposMode.input.value}
                    showValidationErrorsOnMount={submitted}
                    onLiveChange={onSeriesLiveChange}
                    onEditSeriesRequest={onEditSeriesRequest}
                    onEditSeriesCommit={onEditSeriesCommit}
                    onEditSeriesCancel={onEditSeriesCancel}
                    onSeriesRemove={onSeriesRemove}
                />
            </FormGroup>

            <hr className={styles.creationInsightFormSeparator} />

            <FormGroup name="chart settings group" title="Chart settings">
                <FormInput
                    title="Title"
                    required={true}
                    description="Shown as the title for your insight"
                    placeholder="Example: Migration to React function components"
                    valid={title.meta.touched && title.meta.validState === 'VALID'}
                    error={title.meta.touched && title.meta.error}
                    {...title.input}
                    className="d-flex flex-column"
                />

                {!isGqlBackend && (
                    <VisibilityPicker
                        subjects={subjects}
                        value={visibility.input.value}
                        labelClassName={styles.creationInsightFormGroupLabel}
                        onChange={visibility.input.onChange}
                    />
                )}

                <FormGroup
                    name="insight step group"
                    title="Granularity: distance between data points"
                    description="The prototype supports 7 datapoints, so your total x-axis timeframe is 6 times the distance between each point."
                    error={stepValue.meta.touched && stepValue.meta.error}
                    className="mt-4"
                    labelClassName={styles.creationInsightFormGroupLabel}
                    contentClassName="d-flex flex-wrap mb-n2"
                >
                    <FormInput
                        placeholder="ex. 2"
                        required={true}
                        type="number"
                        min={1}
                        {...stepValue.input}
                        valid={stepValue.meta.touched && stepValue.meta.validState === 'VALID'}
                        errorInputState={stepValue.meta.touched && stepValue.meta.validState === 'INVALID'}
                        className={classNames(styles.creationInsightFormStepInput)}
                    />

                    <FormRadioInput
                        title="Hours"
                        name="step"
                        value="hours"
                        checked={step.input.value === 'hours'}
                        onChange={step.input.onChange}
                        disabled={step.input.disabled}
                        className="mr-3"
                    />
                    <FormRadioInput
                        title="Days"
                        name="step"
                        value="days"
                        checked={step.input.value === 'days'}
                        onChange={step.input.onChange}
                        disabled={step.input.disabled}
                        className="mr-3"
                    />
                    <FormRadioInput
                        title="Weeks"
                        name="step"
                        value="weeks"
                        checked={step.input.value === 'weeks'}
                        onChange={step.input.onChange}
                        disabled={step.input.disabled}
                        className="mr-3"
                    />
                    <FormRadioInput
                        title="Months"
                        name="step"
                        value="months"
                        checked={step.input.value === 'months'}
                        onChange={step.input.onChange}
                        disabled={step.input.disabled}
                        className="mr-3"
                    />
                    <FormRadioInput
                        title="Years"
                        name="step"
                        value="years"
                        checked={step.input.value === 'years'}
                        onChange={step.input.onChange}
                        disabled={step.input.disabled}
                        className="mr-3"
                    />
                </FormGroup>
            </FormGroup>

            <hr className={styles.creationInsightFormSeparator} />

            <div className="d-flex flex-wrap align-items-center">
                {submitErrors?.[FORM_ERROR] && <ErrorAlert className="w-100" error={submitErrors[FORM_ERROR]} />}

                <LoaderButton
                    alwaysShowLabel={true}
                    loading={submitting}
                    label={submitting ? 'Submitting' : isEditMode ? 'Save insight' : 'Create code insight'}
                    type="submit"
                    disabled={submitting}
                    data-testid="insight-save-button"
                    className="btn btn-primary mr-2 mb-2"
                />

                <Button type="button" variant="secondary" outline={true} className="mb-2 mr-auto" onClick={onCancel}>
                    Cancel
                </Button>

                <Button
                    type="reset"
                    disabled={!isFormClearActive}
                    variant="secondary"
                    outline={true}
                    className="border-0"
                >
                    Clear all fields
                </Button>
            </div>
        </form>
    )
}
