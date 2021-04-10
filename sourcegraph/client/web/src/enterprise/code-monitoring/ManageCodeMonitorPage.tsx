import * as H from 'history'
import React, { useEffect } from 'react'
import { RouteComponentProps } from 'react-router'
import { Observable } from 'rxjs'
import { startWith, catchError, tap } from 'rxjs/operators'

import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { asError, isErrorLike } from '@sourcegraph/shared/src/util/errors'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'

import { AuthenticatedUser } from '../../auth'
import { PageTitle } from '../../components/PageTitle'
import { CodeMonitorFields, MonitorEmailPriority } from '../../graphql-operations'
import { eventLogger } from '../../tracking/eventLogger'

import {
    fetchCodeMonitor as _fetchCodeMonitor,
    updateCodeMonitor as _updateCodeMonitor,
    deleteCodeMonitor as _deleteCodeMonitor,
} from './backend'
import { CodeMonitorForm } from './components/CodeMonitorForm'

export interface ManageCodeMonitorPageProps extends RouteComponentProps<{ id: Scalars['ID'] }> {
    authenticatedUser: AuthenticatedUser
    location: H.Location
    history: H.History

    fetchCodeMonitor?: typeof _fetchCodeMonitor
    updateCodeMonitor?: typeof _updateCodeMonitor
    deleteCodeMonitor?: typeof _deleteCodeMonitor
}

export const ManageCodeMonitorPage: React.FunctionComponent<ManageCodeMonitorPageProps> = ({
    authenticatedUser,
    history,
    location,
    match,
    fetchCodeMonitor = _fetchCodeMonitor,
    updateCodeMonitor = _updateCodeMonitor,
    deleteCodeMonitor = _deleteCodeMonitor,
}) => {
    const LOADING = 'loading' as const

    useEffect(() => eventLogger.logViewEvent('ManageCodeMonitorPage'), [])

    const [codeMonitorState, setCodeMonitorState] = React.useState<CodeMonitorFields>({
        id: '',
        description: '',
        enabled: true,
        trigger: { id: '', query: '' },
        actions: { nodes: [{ id: '', enabled: true, recipients: { nodes: [{ id: authenticatedUser.id }] } }] },
    })

    const codeMonitorOrError = useObservable(
        React.useMemo(
            () =>
                fetchCodeMonitor(match.params.id).pipe(
                    tap(monitor => {
                        if (monitor.node !== null) {
                            setCodeMonitorState(monitor.node)
                        }
                    }),
                    startWith(LOADING),
                    catchError(error => [asError(error)])
                ),
            [match.params.id, fetchCodeMonitor]
        )
    )

    const updateMonitorRequest = React.useCallback(
        (codeMonitor: CodeMonitorFields): Observable<Partial<CodeMonitorFields>> =>
            updateCodeMonitor(
                {
                    id: match.params.id,
                    update: {
                        namespace: authenticatedUser.id,
                        description: codeMonitor.description,
                        enabled: codeMonitor.enabled,
                    },
                },
                { id: codeMonitor.trigger.id, update: { query: codeMonitor.trigger.query } },
                codeMonitor.actions.nodes.map(action => ({
                    email: {
                        id: action.id,
                        update: {
                            enabled: action.enabled,
                            priority: MonitorEmailPriority.NORMAL,
                            recipients: [authenticatedUser.id],
                            header: '',
                        },
                    },
                }))
            ),
        [authenticatedUser.id, match.params.id, updateCodeMonitor]
    )

    return (
        <div className="container col-8">
            <PageTitle title="Manage code monitor" />
            <div className="page-header d-flex flex-wrap align-items-center">
                <h2 className="flex-grow-1">Manage code monitor</h2>
            </div>
            Code monitors watch your code for specific triggers and run actions in response.{' '}
            <a href="https://docs.sourcegraph.com/code_monitoring" target="_blank" rel="noopener">
                Learn more
            </a>
            {codeMonitorOrError === 'loading' && <LoadingSpinner className="icon-inline" />}
            {codeMonitorOrError && !isErrorLike(codeMonitorOrError) && codeMonitorOrError !== 'loading' && (
                <>
                    <CodeMonitorForm
                        history={history}
                        location={location}
                        authenticatedUser={authenticatedUser}
                        deleteCodeMonitor={deleteCodeMonitor}
                        onSubmit={updateMonitorRequest}
                        codeMonitor={codeMonitorState}
                        submitButtonLabel="Save"
                        showDeleteButton={true}
                    />
                </>
            )}
        </div>
    )
}
