import classNames from 'classnames'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import ChevronDownIcon from 'mdi-react/ChevronDownIcon'
import ChevronRightIcon from 'mdi-react/ChevronRightIcon'
import React, { useCallback, useMemo, useState } from 'react'

import { Button } from '@sourcegraph/wildcard'

import { Timestamp } from '../../../../components/time/Timestamp'
import { EventStatus, MonitorActionEvents, MonitorTriggerEventWithActions } from '../../../../graphql-operations'

import { CollapsibleDetailsWithStatus } from './CollapsibleDetailsWithStatus'
import styles from './TriggerEvent.module.scss'

export const TriggerEvent: React.FunctionComponent<{
    triggerEvent: MonitorTriggerEventWithActions
    startOpen?: boolean
}> = ({ triggerEvent, startOpen = false }) => {
    const [expanded, setExpanded] = useState(startOpen)

    const toggleExpanded = useCallback(() => setExpanded(expanded => !expanded), [])

    // Either there's an error in the trigger itself, or in any of the actions.
    const hasError = useMemo(
        () =>
            triggerEvent.status === EventStatus.ERROR ||
            triggerEvent.actions.nodes.some(action =>
                action.events.nodes.some(actionEvent => actionEvent.status === EventStatus.ERROR)
            ),
        [triggerEvent]
    )

    function getTriggerEventMessage(): string {
        if (triggerEvent.message) {
            return triggerEvent.message
        }

        switch (triggerEvent.status) {
            case EventStatus.ERROR:
                return 'Unknown error occurred when running the search'
            case EventStatus.PENDING:
                return 'Search is pending'
            default:
                return 'Search ran successfully'
        }
    }

    return (
        <>
            <Button onClick={toggleExpanded} className={classNames('btn-icon d-block', styles.expandButton)}>
                {expanded ? (
                    <ChevronDownIcon className="icon-inline mr-2" />
                ) : (
                    <ChevronRightIcon className="icon-inline mr-2" />
                )}

                {hasError ? <AlertCircleIcon className={classNames(styles.errorIcon, 'icon-inline mr-2')} /> : <span />}

                <span>
                    Run <Timestamp date={triggerEvent.timestamp} />
                </span>
            </Button>

            {expanded && (
                <>
                    <CollapsibleDetailsWithStatus
                        status={triggerEvent.status}
                        message={getTriggerEventMessage()}
                        title="Monitor trigger"
                        startOpen={startOpen}
                    />

                    {triggerEvent.actions.nodes.map(action => (
                        <>
                            {action.events.nodes.map(actionEvent => (
                                <CollapsibleDetailsWithStatus
                                    key={actionEvent.id}
                                    status={actionEvent.status}
                                    message={getActionEventMessage(actionEvent)}
                                    title={getActionEventTitle(action)}
                                    startOpen={startOpen}
                                />
                            ))}

                            {action.events.nodes.length === 0 && (
                                <CollapsibleDetailsWithStatus
                                    status="skipped"
                                    message="This action was not run because it was disabled or there were no new results."
                                    title={getActionEventTitle(action)}
                                    startOpen={startOpen}
                                />
                            )}
                        </>
                    ))}
                </>
            )}
        </>
    )
}

function getActionEventMessage(actionEvent: MonitorActionEvents['nodes'][number]): string {
    if (actionEvent.message) {
        return actionEvent.message
    }

    switch (actionEvent.status) {
        case EventStatus.ERROR:
            return 'Unknown error occurred when sending the notification'
        case EventStatus.PENDING:
            return 'Notification is pending'
        default:
            return 'Notification sent successfully'
    }
}

function getActionEventTitle(action: MonitorTriggerEventWithActions['actions']['nodes'][number]): string {
    switch (action.__typename) {
        case 'MonitorEmail':
            return 'Email'
        case 'MonitorSlackWebhook':
            return 'Slack'
        case 'MonitorWebhook':
            return 'Webhook'
    }
}
