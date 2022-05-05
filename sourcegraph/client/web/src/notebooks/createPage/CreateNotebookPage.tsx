import React, { useMemo } from 'react'

import { Redirect } from 'react-router'
import { catchError, startWith } from 'rxjs/operators'

import { asError, isErrorLike } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { LoadingSpinner, useObservable, Alert } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../auth'
import { Page } from '../../components/Page'
import { PageRoutes } from '../../routes.constants'
import { createNotebook } from '../backend'

const LOADING = 'loading' as const

export const CreateNotebookPage: React.FunctionComponent<
    React.PropsWithChildren<TelemetryProps & { authenticatedUser: AuthenticatedUser }>
> = ({ telemetryService, authenticatedUser }) => {
    const notebookOrError = useObservable(
        useMemo(
            () =>
                createNotebook({
                    notebook: { title: 'New Notebook', blocks: [], public: false, namespace: authenticatedUser.id },
                }).pipe(
                    startWith(LOADING),
                    catchError(error => [asError(error)])
                ),
            [authenticatedUser]
        )
    )

    if (notebookOrError && !isErrorLike(notebookOrError) && notebookOrError !== LOADING) {
        telemetryService.log('SearchNotebookCreated')
        return <Redirect to={PageRoutes.Notebook.replace(':id', notebookOrError.id)} />
    }

    return (
        <Page>
            {notebookOrError === LOADING && (
                <div className="d-flex justify-content-center">
                    <LoadingSpinner />
                </div>
            )}
            {isErrorLike(notebookOrError) && (
                <Alert variant="danger">
                    Error while creating the notebook: <strong>{notebookOrError.message}</strong>
                </Alert>
            )}
        </Page>
    )
}
