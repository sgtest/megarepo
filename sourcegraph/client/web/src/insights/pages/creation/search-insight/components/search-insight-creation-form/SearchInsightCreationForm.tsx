import classnames from 'classnames'
import React, { FormEventHandler, RefObject } from 'react'

import { ErrorAlert } from '../../../../../../components/alerts'
import { LoaderButton } from '../../../../../../components/LoaderButton'
import { FormGroup } from '../../../../../components/form/form-group/FormGroup'
import { FormInput } from '../../../../../components/form/form-input/FormInput'
import { FormRadioInput } from '../../../../../components/form/form-radio-input/FormRadioInput'
import { useFieldAPI } from '../../../../../components/form/hooks/useField'
import { FORM_ERROR, SubmissionErrors } from '../../../../../components/form/hooks/useForm'
import { DataSeries } from '../../../../../core/backend/types'
import { CreateInsightFormFields } from '../../types'
import { FormSeries } from '../form-series/FormSeries'

import styles from './SearchInsightCreationForm.module.scss'

interface CreationSearchInsightFormProps {
    /** This component might be used in edit or creation insight case. */
    mode?: 'creation' | 'edit'

    innerRef: RefObject<any>
    handleSubmit: FormEventHandler
    submitErrors: SubmissionErrors
    submitting: boolean
    className?: string

    title: useFieldAPI<CreateInsightFormFields['title']>
    repositories: useFieldAPI<CreateInsightFormFields['repositories']>
    visibility: useFieldAPI<CreateInsightFormFields['visibility']>
    series: useFieldAPI<CreateInsightFormFields['series']>
    step: useFieldAPI<CreateInsightFormFields['step']>
    stepValue: useFieldAPI<CreateInsightFormFields['stepValue']>

    onCancel: () => void

    /**
     * Edit series array used below for rendering series edit form.
     * In case of some element has undefined value we're showing
     * series card with data instead of form.
     * */
    editSeries: (CreateInsightFormFields['series'][number] | undefined)[]

    /**
     * Handler to listen latest value form particular series edit form
     * Used to get information for live preview chart.
     * */
    onSeriesLiveChange: (liveSeries: DataSeries, isValid: boolean, index: number) => void

    /**
     * Handlers for CRUD operation over series. Add, delete, update and cancel
     * series edit form.
     * */
    onEditSeriesRequest: (openedCardIndex: number) => void
    onEditSeriesCommit: (seriesIndex: number, editedSeries: DataSeries) => void
    onEditSeriesCancel: (closedCardIndex: number) => void
    onSeriesRemove: (removedSeriesIndex: number) => void
}

/**
 * Displays creation code insight form (title, visibility, series, etc.)
 * UI layer only, all controlled data should be managed by consumer of this component.
 * */
export const SearchInsightCreationForm: React.FunctionComponent<CreationSearchInsightFormProps> = props => {
    const {
        mode,
        innerRef,
        handleSubmit,
        submitErrors,
        submitting,
        title,
        repositories,
        visibility,
        series,
        editSeries,
        stepValue,
        step,
        className,
        onCancel,
        onSeriesLiveChange,
        onEditSeriesRequest,
        onEditSeriesCommit,
        onEditSeriesCancel,
        onSeriesRemove,
    } = props

    const isEditMode = mode === 'edit'

    return (
        // eslint-disable-next-line react/forbid-elements
        <form
            noValidate={true}
            ref={innerRef}
            onSubmit={handleSubmit}
            className={classnames(className, 'd-flex flex-column')}
        >
            <FormInput
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

            <FormInput
                title="Repositories"
                required={true}
                description="Create a list of repositories to run your search over. Separate them with commas."
                placeholder="Add or search for repositories"
                loading={repositories.meta.validState === 'CHECKING'}
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
                <FormSeries
                    series={series.input.value}
                    editSeries={editSeries}
                    onLiveChange={onSeriesLiveChange}
                    onEditSeriesRequest={onEditSeriesRequest}
                    onEditSeriesCommit={onEditSeriesCommit}
                    onEditSeriesCancel={onEditSeriesCancel}
                    onSeriesRemove={onSeriesRemove}
                />
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
                <FormInput
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
                {submitErrors?.[FORM_ERROR] && <ErrorAlert error={submitErrors[FORM_ERROR]} />}

                <LoaderButton
                    alwaysShowLabel={true}
                    loading={submitting}
                    label={submitting ? 'Submitting' : isEditMode ? 'Edit insight' : 'Create code insight'}
                    type="submit"
                    disabled={submitting}
                    className="btn btn-primary mr-2"
                />

                <button type="button" className="btn btn-outline-secondary" onClick={onCancel}>
                    Cancel
                </button>
            </div>
        </form>
    )
}
