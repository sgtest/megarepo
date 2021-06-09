import classnames from 'classnames'
import React from 'react'
import { noop } from 'rxjs'

import { Settings } from '@sourcegraph/shared/src/settings/settings'

import { useField } from '../../../../../components/form/hooks/useField'
import { FormChangeEvent, SubmissionErrors, useForm } from '../../../../../components/form/hooks/useForm'
import { useTitleValidator } from '../../../../../components/form/hooks/useTitleValidator'
import { Organization } from '../../../../../components/visibility-picker/VisibilityPicker'
import { InsightTypePrefix } from '../../../../../core/types'
import { LangStatsCreationFormFields } from '../../types'
import { LangStatsInsightCreationForm } from '../lang-stats-insight-creation-form/LangStatsInsightCreationForm'
import { LangStatsInsightLivePreview } from '../live-preview-chart/LangStatsInsightLivePreview'

import styles from './LangStatsInsightCreationContent.module.scss'
import { repositoriesFieldValidator, repositoryFieldAsyncValidator, thresholdFieldValidator } from './validators'

const INITIAL_VALUES: LangStatsCreationFormFields = {
    repository: '',
    title: '',
    threshold: 3,
    visibility: 'personal',
}

export interface LangStatsInsightCreationContentProps {
    /**
     * This component might be used in two different modes for creation and
     * edit mode. In edit mode we change some text keys for form and trigger
     * validation on form fields immediately.
     * */
    mode?: 'creation' | 'edit'
    /** Final settings cascade. Used for title field validation. */
    settings?: Settings | null

    organizations?: Organization[]

    /** Initial value for all form fields. */
    initialValues?: LangStatsCreationFormFields
    /** Custom class name for root form element. */
    className?: string
    /** Submit handler for form element. */
    onSubmit: (values: LangStatsCreationFormFields) => SubmissionErrors | Promise<SubmissionErrors> | void
    /** Cancel handler. */
    onCancel?: () => void
    /** Change handlers is called every time when user changed any field within the form. */
    onChange?: (event: FormChangeEvent<LangStatsCreationFormFields>) => void
}

export const LangStatsInsightCreationContent: React.FunctionComponent<LangStatsInsightCreationContentProps> = props => {
    const {
        mode = 'creation',
        settings,
        organizations = [],
        initialValues = INITIAL_VALUES,
        className,
        onSubmit,
        onCancel = noop,
        onChange = noop,
    } = props

    const { values, handleSubmit, formAPI, ref } = useForm<LangStatsCreationFormFields>({
        initialValues,
        onSubmit,
        onChange,
        touched: mode === 'edit',
    })

    // We can't have two or more insights with the same name, since we rely on name as on id of insights.
    const titleValidator = useTitleValidator({ settings, insightType: InsightTypePrefix.langStats })

    const repository = useField('repository', formAPI, {
        sync: repositoriesFieldValidator,
        async: repositoryFieldAsyncValidator,
    })
    const title = useField('title', formAPI, { sync: titleValidator })
    const threshold = useField('threshold', formAPI, { sync: thresholdFieldValidator })
    const visibility = useField('visibility', formAPI)

    // If some fields that needed to run live preview  are invalid
    // we should disabled live chart preview
    const allFieldsForPreviewAreValid =
        repository.meta.validState === 'VALID' ||
        (repository.meta.validState === 'CHECKING' && threshold.meta.validState === 'VALID')

    const handleFormReset = (): void => {
        // TODO [VK] Change useForm API in order to implement form.reset method.
        title.input.onChange('')
        repository.input.onChange('')
        // Focus first element of the form
        repository.input.ref.current?.focus()
        visibility.input.onChange('personal')
        threshold.input.onChange(3)
    }

    const hasFilledValue = values.repository !== '' || values.title !== ''

    return (
        <div data-testid="code-stats-insight-creation-page-content" className={classnames(styles.content, className)}>
            <LangStatsInsightCreationForm
                mode={mode}
                innerRef={ref}
                handleSubmit={handleSubmit}
                submitErrors={formAPI.submitErrors}
                submitting={formAPI.submitting}
                title={title}
                repository={repository}
                threshold={threshold}
                visibility={visibility}
                organizations={organizations}
                isFormClearActive={hasFilledValue}
                onCancel={onCancel}
                className={styles.contentForm}
                onFormReset={handleFormReset}
            />

            <LangStatsInsightLivePreview
                repository={repository.meta.value}
                threshold={threshold.meta.value}
                disabled={!allFieldsForPreviewAreValid}
                className={styles.contentLivePreview}
            />
        </div>
    )
}
