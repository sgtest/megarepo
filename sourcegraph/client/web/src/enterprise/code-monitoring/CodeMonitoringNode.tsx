import * as H from 'history'
import React, { useState, useCallback, useMemo } from 'react'
import { Observable, concat, of } from 'rxjs'
import { switchMap, catchError, startWith, takeUntil, tap, delay, mergeMap } from 'rxjs/operators'

import { Toggle } from '@sourcegraph/branded/src/components/Toggle'
import { ErrorLike, isErrorLike, asError } from '@sourcegraph/common'
import { Link } from '@sourcegraph/shared/src/components/Link'
import { Button, LoadingSpinner, useEventObservable } from '@sourcegraph/wildcard'

import { CodeMonitorFields, ToggleCodeMonitorEnabledResult } from '../../graphql-operations'

import { sendTestEmail, toggleCodeMonitorEnabled as _toggleCodeMonitorEnabled } from './backend'
import styles from './CodeMonitoringNode.module.scss'

export interface CodeMonitorNodeProps {
    node: CodeMonitorFields
    location: H.Location
    isSiteAdminUser: boolean
    showCodeMonitoringTestEmailButton: boolean

    toggleCodeMonitorEnabled?: typeof _toggleCodeMonitorEnabled
}

const LOADING = 'LOADING' as const

export const CodeMonitorNode: React.FunctionComponent<CodeMonitorNodeProps> = ({
    location,
    node,
    isSiteAdminUser,
    showCodeMonitoringTestEmailButton,
    toggleCodeMonitorEnabled = _toggleCodeMonitorEnabled,
}: CodeMonitorNodeProps) => {
    const [enabled, setEnabled] = useState<boolean>(node.enabled)

    const [toggleMonitor, toggleMonitorOrError] = useEventObservable(
        useCallback(
            (click: Observable<React.MouseEvent>) =>
                click.pipe(
                    switchMap(() => {
                        const toggleMonitor = toggleCodeMonitorEnabled(node.id, !enabled).pipe(
                            tap(
                                (
                                    idAndEnabled:
                                        | typeof LOADING
                                        | ErrorLike
                                        | ToggleCodeMonitorEnabledResult['toggleCodeMonitor']
                                ) => {
                                    if (idAndEnabled !== LOADING && !isErrorLike(idAndEnabled)) {
                                        setEnabled(idAndEnabled.enabled)
                                    }
                                }
                            ),
                            catchError(error => [asError(error)])
                        )
                        return concat(
                            of(LOADING).pipe(startWith(enabled), delay(300), takeUntil(toggleMonitor)),
                            toggleMonitor
                        )
                    })
                ),
            [node, enabled, setEnabled, toggleCodeMonitorEnabled]
        )
    )

    const [sendEmailRequest] = useEventObservable(
        useCallback(
            (click: Observable<React.MouseEvent<HTMLButtonElement>>) =>
                click.pipe(
                    mergeMap(() =>
                        sendTestEmail(node.trigger.id).pipe(
                            startWith(LOADING),
                            catchError(error => [asError(error)])
                        )
                    )
                ),
            [node]
        )
    )

    const hasEnabledAction = useMemo(() => node.actions.nodes.filter(node => node.enabled).length > 0, [node.actions])

    return (
        <div className={styles.codeMonitoringNode}>
            <div className="d-flex justify-content-between align-items-center">
                <div className="d-flex flex-column">
                    <div className="font-weight-bold">
                        <Link to={`${location.pathname}/${node.id}`}>{node.description}</Link>
                    </div>
                    {/** TODO: Generate this text based on the type of action when new actions are added. */}
                    {node.actions.nodes.length > 0 && (
                        <div className="d-flex text-muted">
                            New search result → Sends email notifications{' '}
                            {showCodeMonitoringTestEmailButton && isSiteAdminUser && hasEnabledAction && node.enabled && (
                                <Button className="p-0 border-0 ml-2" onClick={sendEmailRequest} variant="link">
                                    Send test email
                                </Button>
                            )}
                        </div>
                    )}
                </div>
                <div className="d-flex">
                    {toggleMonitorOrError === LOADING && <LoadingSpinner className="mr-2" />}
                    <div className={styles.toggleWrapper} data-testid="toggle-monitor-enabled">
                        <Toggle
                            onClick={toggleMonitor}
                            value={enabled}
                            className="mr-3"
                            disabled={toggleMonitorOrError === LOADING}
                        />
                    </div>
                    <Button
                        to={`${location.pathname}/${node.id}`}
                        className={styles.editButton}
                        variant="link"
                        as={Link}
                    >
                        Edit
                    </Button>
                </div>
            </div>
            {isErrorLike(toggleMonitorOrError) && (
                <div className="alert alert-danger">Failed to toggle monitor: {toggleMonitorOrError.message}</div>
            )}
        </div>
    )
}
