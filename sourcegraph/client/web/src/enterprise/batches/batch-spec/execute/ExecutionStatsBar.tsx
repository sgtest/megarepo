import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import CheckBoldIcon from 'mdi-react/CheckBoldIcon'
import CircleOffOutlineIcon from 'mdi-react/CircleOffOutlineIcon'
import TimelineClockOutlineIcon from 'mdi-react/TimelineClockOutlineIcon'
import TimerSandIcon from 'mdi-react/TimerSandIcon'

import { pluralize } from '@sourcegraph/common'
import { Icon } from '@sourcegraph/wildcard'

import { BatchSpecWorkspaceStats } from '../../../../graphql-operations'

import styles from './ExecutionStatsBar.module.scss'

export const ExecutionStatsBar: React.FunctionComponent<React.PropsWithChildren<BatchSpecWorkspaceStats>> = stats => (
    <div className="d-flex align-items-center">
        <ExecutionStat>
            <Icon role="img" aria-hidden={true} as={AlertCircleIcon} className="text-danger" />
            {stats.errored} {pluralize('error', stats.errored)}
        </ExecutionStat>
        <ExecutionStat>
            <Icon role="img" aria-hidden={true} as={CheckBoldIcon} className="text-success" />
            {stats.completed} complete
        </ExecutionStat>
        <ExecutionStat>
            <Icon role="img" aria-hidden={true} as={TimerSandIcon} />
            {stats.processing} working
        </ExecutionStat>
        <ExecutionStat>
            <Icon role="img" aria-hidden={true} as={TimelineClockOutlineIcon} />
            {stats.queued} queued
        </ExecutionStat>
        <ExecutionStat>
            <Icon role="img" aria-hidden={true} as={CircleOffOutlineIcon} />
            {stats.ignored} ignored
        </ExecutionStat>
    </div>
)

export const ExecutionStat: React.FunctionComponent<React.PropsWithChildren<{}>> = ({ children }) => (
    <div className={styles.stat}>{children}</div>
)
