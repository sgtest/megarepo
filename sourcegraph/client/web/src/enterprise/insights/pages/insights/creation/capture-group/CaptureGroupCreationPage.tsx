import { FC, useEffect, useMemo } from 'react'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Link, PageHeader, useObservable } from '@sourcegraph/wildcard'

import { PageTitle } from '../../../../../../components/PageTitle'
import { CodeInsightsIcon } from '../../../../../../insights/Icons'
import {
    CodeInsightCreationMode,
    CodeInsightsCreationActions,
    CodeInsightsPage,
    FORM_ERROR,
    FormChangeEvent,
    SubmissionErrors,
} from '../../../../components'
import { MinimalCaptureGroupInsightData } from '../../../../core'
import { useUiFeatures } from '../../../../hooks'
import { CodeInsightTrackType } from '../../../../pings'

import { CaptureGroupCreationContent } from './components/CaptureGroupCreationContent'
import { useCaptureInsightInitialValues } from './hooks/use-capture-insight-initial-values'
import { CaptureGroupFormFields } from './types'
import { getSanitizedCaptureGroupInsight } from './utils/capture-group-insight-sanitizer'

interface CaptureGroupCreationPageProps extends TelemetryProps {
    onInsightCreateRequest: (event: { insight: MinimalCaptureGroupInsightData }) => Promise<unknown>
    onSuccessfulCreation: () => void
    onCancel: () => void
}

export const CaptureGroupCreationPage: FC<CaptureGroupCreationPageProps> = props => {
    const { telemetryService, onInsightCreateRequest, onSuccessfulCreation, onCancel } = props

    const { licensed, insight } = useUiFeatures()
    const creationPermission = useObservable(useMemo(() => insight.getCreationPermissions(), [insight]))

    const [initialFormValues, setInitialFormValues] = useCaptureInsightInitialValues()

    useEffect(() => {
        telemetryService.logViewEvent('CodeInsightsCaptureGroupCreationPage')
    }, [telemetryService])

    const handleSubmit = async (values: CaptureGroupFormFields): Promise<SubmissionErrors | void> => {
        const insight = getSanitizedCaptureGroupInsight(values)

        await onInsightCreateRequest({ insight })

        setInitialFormValues(undefined)
        telemetryService.log('CodeInsightsCaptureGroupCreationPageSubmitClick')
        telemetryService.log(
            'InsightAddition',
            { insightType: CodeInsightTrackType.CaptureGroupInsight },
            { insightType: CodeInsightTrackType.CaptureGroupInsight }
        )

        onSuccessfulCreation()
    }

    const handleCancel = (): void => {
        // Clear initial values if user successfully created search insight
        setInitialFormValues(undefined)
        telemetryService.log('CodeInsightsCaptureGroupCreationPageCancelClick')

        onCancel()
    }

    const handleChange = (event: FormChangeEvent<CaptureGroupFormFields>): void => {
        setInitialFormValues(event.values)
    }

    return (
        <CodeInsightsPage>
            <PageTitle title="Create insight - Code Insights" />

            <PageHeader
                className="mb-5"
                path={[{ icon: CodeInsightsIcon }, { text: 'Create new capture group insight' }]}
                description={
                    <span className="text-muted">
                        Capture group code insights analyze your code based on generated data series queries.{' '}
                        <Link
                            to="/help/code_insights/explanations/automatically_generated_data_series"
                            target="_blank"
                            rel="noopener"
                        >
                            Learn more.
                        </Link>
                    </span>
                }
            />

            <CaptureGroupCreationContent
                touched={false}
                initialValues={initialFormValues}
                className="pb-5"
                onSubmit={handleSubmit}
                onCancel={handleCancel}
                onChange={handleChange}
            >
                {form => (
                    <CodeInsightsCreationActions
                        mode={CodeInsightCreationMode.Creation}
                        licensed={licensed}
                        available={creationPermission?.available}
                        submitting={form.submitting}
                        errors={form.submitErrors?.[FORM_ERROR]}
                        clear={form.isFormClearActive}
                        onCancel={handleCancel}
                    />
                )}
            </CaptureGroupCreationContent>
        </CodeInsightsPage>
    )
}
