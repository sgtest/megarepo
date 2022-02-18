import classNames from 'classnames'
import React, { useContext, useMemo } from 'react'
import { useHistory } from 'react-router-dom'

import { asError } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { PageHeader, Container, Button, LoadingSpinner, useObservable, Link } from '@sourcegraph/wildcard'

import { LoaderButton } from '../../../../../components/LoaderButton'
import { Page } from '../../../../../components/Page'
import { PageTitle } from '../../../../../components/PageTitle'
import { CodeInsightsIcon } from '../../../components'
import { FORM_ERROR, SubmissionErrors } from '../../../components/form/hooks/useForm'
import { CodeInsightsBackendContext } from '../../../core/backend/code-insights-backend-context'

import {
    DashboardCreationFields,
    InsightsDashboardCreationContent,
} from './components/InsightsDashboardCreationContent'
import styles from './InsightsDashboardCreationPage.module.scss'

interface InsightsDashboardCreationPageProps extends TelemetryProps {}

export const InsightsDashboardCreationPage: React.FunctionComponent<InsightsDashboardCreationPageProps> = props => {
    const { telemetryService } = props

    const history = useHistory()
    const { createDashboard, getDashboardSubjects } = useContext(CodeInsightsBackendContext)

    const subjects = useObservable(useMemo(() => getDashboardSubjects(), [getDashboardSubjects]))

    const handleSubmit = async (values: DashboardCreationFields): Promise<SubmissionErrors> => {
        try {
            const createdDashboard = await createDashboard(values).toPromise()

            telemetryService.log('CodeInsightsDashboardCreationPageSubmitClick')

            // Navigate user to the dashboard page with new created dashboard
            history.push(`/insights/dashboards/${createdDashboard.id}`)
        } catch (error) {
            return { [FORM_ERROR]: asError(error) }
        }

        return
    }

    const handleCancel = (): void => history.goBack()

    // Loading state
    if (subjects === undefined) {
        return <LoadingSpinner />
    }

    return (
        <Page className={classNames('col-8', styles.page)}>
            <PageTitle title="Add new dashboard" />

            <PageHeader path={[{ icon: CodeInsightsIcon }, { text: 'Add new dashboard' }]} />

            <span className="text-muted d-block mt-2">
                Dashboards group your insights and let you share them with others.{' '}
                <Link to="/help/code_insights/explanations/viewing_code_insights" target="_blank" rel="noopener">
                    Learn more.
                </Link>
            </span>

            <Container className="mt-4">
                <InsightsDashboardCreationContent subjects={subjects} onSubmit={handleSubmit}>
                    {formAPI => (
                        <>
                            <Button
                                type="button"
                                variant="secondary"
                                outline={true}
                                className="mb-2"
                                onClick={handleCancel}
                            >
                                Cancel
                            </Button>

                            <LoaderButton
                                alwaysShowLabel={true}
                                data-testid="insight-save-button"
                                loading={formAPI.submitting}
                                label={formAPI.submitting ? 'Adding' : 'Add dashboard'}
                                type="submit"
                                disabled={formAPI.submitting}
                                className="ml-2 mb-2"
                                variant="primary"
                            />
                        </>
                    )}
                </InsightsDashboardCreationContent>
            </Container>
        </Page>
    )
}
