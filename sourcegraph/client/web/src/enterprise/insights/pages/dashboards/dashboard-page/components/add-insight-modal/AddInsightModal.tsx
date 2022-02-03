import { VisuallyHidden } from '@reach/visually-hidden'
import CloseIcon from 'mdi-react/CloseIcon'
import React, { useContext, useMemo } from 'react'

import { asError } from '@sourcegraph/common'
import { Button, LoadingSpinner, useObservable, Modal } from '@sourcegraph/wildcard'

import { FORM_ERROR, SubmissionErrors } from '../../../../../components/form/hooks/useForm'
import { CodeInsightsBackendContext } from '../../../../../core/backend/code-insights-backend-context'
import { parseDashboardScope } from '../../../../../core/backend/utils/parse-dashboard-scope'
import { CustomInsightDashboard } from '../../../../../core/types'
import { isGlobalSubject, SupportedInsightSubject } from '../../../../../core/types/subjects'

import styles from './AddInsightModal.module.scss'
import {
    AddInsightFormValues,
    AddInsightModalContent,
} from './components/add-insight-modal-content/AddInsightModalContent'

export interface AddInsightModalProps {
    dashboard: CustomInsightDashboard
    onClose: () => void
}

export const AddInsightModal: React.FunctionComponent<AddInsightModalProps> = props => {
    const { dashboard, onClose } = props
    const { getReachableInsights, getDashboardSubjects, assignInsightsToDashboard } = useContext(
        CodeInsightsBackendContext
    )

    const subjects = useObservable(useMemo(() => getDashboardSubjects(), [getDashboardSubjects]))
    const insights = useObservable(
        useMemo(() => getReachableInsights({ subjectId: dashboard.owner?.id || '' }), [
            dashboard.owner,
            getReachableInsights,
        ])
    )

    const initialValues = useMemo<AddInsightFormValues>(
        () => ({
            searchInput: '',
            insightIds: dashboard.insightIds ?? [],
        }),
        [dashboard]
    )

    const handleSubmit = async (values: AddInsightFormValues): Promise<void | SubmissionErrors> => {
        try {
            const { insightIds } = values
            const type = dashboard.grants && parseDashboardScope(dashboard.grants)

            await assignInsightsToDashboard({
                id: dashboard.id,
                previousDashboard: dashboard,
                nextDashboardInput: {
                    name: dashboard.title,
                    visibility: getDashboardVisibilityId(dashboard, subjects ?? []),
                    insightIds,
                    type,
                },
            }).toPromise()

            onClose()
        } catch (error) {
            return { [FORM_ERROR]: asError(error) }
        }
    }

    if (insights === undefined) {
        return (
            <Modal className={styles.modal} aria-label="Add insights to dashboard modal">
                <LoadingSpinner inline={false} />
            </Modal>
        )
    }

    return (
        <Modal className={styles.modal} onDismiss={onClose} aria-label="Add insights to dashboard modal">
            <Button variant="icon" className={styles.closeButton} onClick={onClose}>
                <VisuallyHidden>Close</VisuallyHidden>
                <CloseIcon />
            </Button>

            <h2 className="mb-3">
                Add insight to <q>{dashboard.title}</q>
            </h2>

            {!insights.length && <span>There are no insights for this dashboard.</span>}

            {insights.length > 0 && (
                <AddInsightModalContent
                    initialValues={initialValues}
                    insights={insights}
                    onCancel={onClose}
                    onSubmit={handleSubmit}
                />
            )}
        </Modal>
    )
}

function getDashboardVisibilityId(dashboard: CustomInsightDashboard, subjects: SupportedInsightSubject[]): string {
    if (dashboard.owner) {
        return dashboard.owner.id
    }

    if (dashboard.grants) {
        const { users, organizations, global } = dashboard.grants
        const globalSubject = subjects.find(isGlobalSubject)

        if (global && globalSubject) {
            return globalSubject.id
        }

        return users[0] ?? organizations[0] ?? 'unkown'
    }

    return 'unknown subject id'
}
