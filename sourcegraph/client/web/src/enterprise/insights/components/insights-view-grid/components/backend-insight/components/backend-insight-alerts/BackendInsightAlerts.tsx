import React, { FC } from 'react'

import classNames from 'classnames'
import ProgressWrench from 'mdi-react/ProgressWrenchIcon'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { ErrorLike } from '@sourcegraph/common'
import { Alert, H4 } from '@sourcegraph/wildcard'

import { InsightInProcessError } from '../../../../../../core/backend/utils/errors'

import styles from './BackendInsightAlerts.module.scss'

interface BackendAlertOverLayProps {
    isFetchingHistoricalData: boolean
    hasNoData: boolean
    className?: string
}

export const BackendAlertOverlay: FC<BackendAlertOverLayProps> = props => {
    const { isFetchingHistoricalData, hasNoData, className } = props

    if (isFetchingHistoricalData) {
        return (
            <AlertOverlay
                title="This insight is still being processed"
                description="Datapoints shown may be undercounted."
                icon={<ProgressWrench className={classNames('mb-3')} size={33} />}
                className={className}
            />
        )
    }

    if (hasNoData) {
        return (
            <AlertOverlay
                title="No data to display"
                description="We couldn’t find any matches for this insight."
                className={className}
            />
        )
    }

    return null
}

export interface AlertOverlayProps {
    title: string
    description: string
    icon?: React.ReactNode
    className?: string
}

const AlertOverlay: React.FunctionComponent<React.PropsWithChildren<AlertOverlayProps>> = props => {
    const { title, description, icon, className } = props

    return (
        <div className={classNames(className, styles.alertContainer)}>
            {icon && <div className={styles.icon}>{icon}</div>}
            <H4 className={styles.title}>{title}</H4>
            <small className={styles.description}>{description}</small>
        </div>
    )
}

interface BackendInsightErrorAlertProps {
    error: ErrorLike
}

export const BackendInsightErrorAlert: FC<BackendInsightErrorAlertProps> = props =>
    props.error instanceof InsightInProcessError ? (
        <Alert variant="info">{props.error.message}</Alert>
    ) : (
        <ErrorAlert error={props.error} />
    )
