import React, { useCallback, useMemo, useState } from 'react'

import { mdiAlertCircle, mdiCheckBold, mdiOpenInNew, mdiChevronDown, mdiChevronRight } from '@mdi/js'
import classNames from 'classnames'

import { Button, Icon, Link, Tooltip } from '@sourcegraph/wildcard'

import { ConnectionList } from '../../../../components/FilteredConnection/ui'
import { Timestamp } from '../../../../components/time/Timestamp'
import { CodeMonitorWithEvents, EventStatus } from '../../../../graphql-operations'

import { TriggerEvent } from './TriggerEvent'

import styles from './MonitorLogNode.module.scss'

const clickCatcher = (event: React.MouseEvent<HTMLAnchorElement>): void => {
    event.stopPropagation()
}

export const MonitorLogNode: React.FunctionComponent<
    React.PropsWithChildren<{
        monitor: CodeMonitorWithEvents
        now?: () => Date
        startOpen?: boolean
    }>
> = ({ monitor, now, startOpen = false }) => {
    const [expanded, setExpanded] = useState(startOpen)

    const toggleExpanded = useCallback(() => setExpanded(expanded => !expanded), [])

    // Either there's an error in the trigger itself, or in any of the actions.
    const hasError = useMemo(
        () =>
            monitor.trigger.events.nodes.some(
                triggerEvent =>
                    triggerEvent.status === EventStatus.ERROR ||
                    triggerEvent.actions.nodes.some(action =>
                        action.events.nodes.some(actionEvent => actionEvent.status === EventStatus.ERROR)
                    )
            ),
        [monitor]
    )

    // The most recent event is the first one in the list.
    const lastRun = useMemo(
        () => (monitor.trigger.events.nodes.length > 0 ? monitor.trigger.events.nodes[0].timestamp : null),
        [monitor.trigger.events.nodes]
    )

    return (
        <li className={styles.container}>
            <div className="d-flex align-items-center">
                <Button
                    onClick={toggleExpanded}
                    className="btn-icon text-left pl-0 border-0 d-flex align-items-center flex-1"
                >
                    {expanded ? (
                        <Icon
                            className="mr-2 flex-shrink-0"
                            svgPath={mdiChevronDown}
                            inline={false}
                            aria-label="Collapse code monitor"
                        />
                    ) : (
                        <Icon
                            className="mr-2 flex-shrink-0"
                            svgPath={mdiChevronRight}
                            inline={false}
                            aria-label="Expand code monitor"
                        />
                    )}
                    {hasError ? (
                        <Tooltip content="One or more runs of this code monitor have an error" placement="top">
                            <Icon
                                aria-label="One or more runs of this code monitor have an error"
                                svgPath={mdiAlertCircle}
                                className={classNames(styles.errorIcon, 'mr-1 flex-shrink-0')}
                            />
                        </Tooltip>
                    ) : (
                        <Tooltip content="Monitor running as normal" placement="top">
                            <Icon
                                aria-label="Monitor running as normal"
                                svgPath={mdiCheckBold}
                                className={classNames(styles.checkIcon, 'mr-1 flex-shrink-0')}
                            />
                        </Tooltip>
                    )}
                    {monitor.description}
                    {/* Use clickCatcher so clicking on link doesn't expand/collapse row */}
                    <Link
                        to={`/code-monitoring/${monitor.id}`}
                        className="ml-2 font-weight-normal"
                        target="_blank"
                        rel="noopener noreferrer"
                        onClick={clickCatcher}
                    >
                        Monitor details <Icon role="img" aria-hidden={true} svgPath={mdiOpenInNew} />
                    </Link>
                </Button>
                <span className="text-nowrap mr-2">
                    {lastRun ? <Timestamp date={lastRun} now={now} noAbout={true} /> : <>Never</>}
                </span>
            </div>

            {expanded && (
                <div className={styles.expandedRow}>
                    {monitor.trigger.events.nodes.length === 0 ? (
                        <div>This code monitor has not been run yet.</div>
                    ) : (
                        <ConnectionList as="ol">
                            {monitor.trigger.events.nodes.map(triggerEvent => (
                                <TriggerEvent
                                    key={triggerEvent.id}
                                    triggerEvent={triggerEvent}
                                    startOpen={startOpen}
                                    now={now}
                                />
                            ))}
                        </ConnectionList>
                    )}
                </div>
            )}
        </li>
    )
}
