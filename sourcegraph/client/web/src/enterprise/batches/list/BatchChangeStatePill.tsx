import React, { useMemo } from 'react'

import classNames from 'classnames'
import HistoryIcon from 'mdi-react/HistoryIcon'

import { Badge } from '@sourcegraph/wildcard'

import { BatchChangeState, BatchSpecState, Scalars } from '../../../graphql-operations'

import styles from './BatchChangeStatePill.module.scss'

// A batch spec state is actionable if it's not pending, canceling, or canceled.
const actionableBatchSpecStates = [
    BatchSpecState.COMPLETED,
    BatchSpecState.FAILED,
    BatchSpecState.PROCESSING,
    BatchSpecState.QUEUED,
] as const
type ActionableBatchSpecState = typeof actionableBatchSpecStates[number]

const isLatestExecutionActionable = (executionState: BatchSpecState): executionState is ActionableBatchSpecState =>
    actionableBatchSpecStates.includes(executionState as any)

export interface BatchChangeStatePillProps {
    className?: string
    state: BatchChangeState
    latestExecutionState?: BatchSpecState
    currentSpecID: Scalars['ID']
    latestSpecID?: Scalars['ID']
}

export const BatchChangeStatePill: React.FunctionComponent<React.PropsWithChildren<BatchChangeStatePillProps>> = ({
    className,
    state,
    latestExecutionState,
    currentSpecID,
    latestSpecID,
}) => {
    const isCompleteAndApplied = useMemo(
        () => currentSpecID === latestSpecID && latestExecutionState === BatchSpecState.COMPLETED,
        [currentSpecID, latestSpecID, latestExecutionState]
    )

    // We only show the execution status part of the pill if:
    // - the batch change is not closed
    // - the latest execution state is actionable
    // - the latest execution is not already complete and applied
    const executionStatePill =
        latestExecutionState &&
        state !== BatchChangeState.CLOSED &&
        isLatestExecutionActionable(latestExecutionState) &&
        !isCompleteAndApplied ? (
            <ExecutionStatePill latestExecutionState={latestExecutionState} />
        ) : null

    return (
        <div
            role="group"
            className={classNames(styles.pillGroup, className, {
                [styles.open]: state === BatchChangeState.OPEN,
                [styles.draft]: state === BatchChangeState.DRAFT,
                [styles.closed]: state === BatchChangeState.CLOSED,
            })}
        >
            <StatePill state={state} />
            {executionStatePill}
        </div>
    )
}

const StatePill: React.FunctionComponent<React.PropsWithChildren<Pick<BatchChangeStatePillProps, 'state'>>> = ({
    state,
}) => {
    switch (state) {
        case BatchChangeState.OPEN:
            return (
                <Badge variant="success" className={styles.statePill}>
                    Open
                </Badge>
            )
        case BatchChangeState.CLOSED:
            return (
                <Badge variant="danger" className={styles.statePill}>
                    Closed
                </Badge>
            )
        case BatchChangeState.DRAFT:
        default:
            return (
                <Badge variant="secondary" className={styles.statePill}>
                    Draft
                </Badge>
            )
    }
}

const ExecutionStatePill: React.FunctionComponent<
    React.PropsWithChildren<{ latestExecutionState: ActionableBatchSpecState }>
> = ({ latestExecutionState }) => {
    switch (latestExecutionState) {
        case BatchSpecState.PROCESSING:
        case BatchSpecState.QUEUED:
            return (
                <Badge
                    variant="warning"
                    className={styles.executionPill}
                    data-tooltip={`This batch change has a new spec ${
                        latestExecutionState === BatchSpecState.QUEUED
                            ? 'queued for execution'
                            : 'in the process of executing'
                    }.`}
                >
                    <HistoryIcon className={styles.executionIcon} />
                </Badge>
            )

        case BatchSpecState.COMPLETED:
            return (
                <Badge
                    variant="primary"
                    className={styles.executionPill}
                    data-tooltip="This batch change has a newer batch spec execution that is ready to be applied."
                >
                    <HistoryIcon className={styles.executionIcon} />
                </Badge>
            )
        case BatchSpecState.FAILED:
        default:
            return (
                <Badge
                    variant="danger"
                    className={styles.executionPill}
                    data-tooltip="The latest batch spec execution for this batch change failed."
                >
                    <HistoryIcon className={styles.executionIcon} />
                </Badge>
            )
    }
}
