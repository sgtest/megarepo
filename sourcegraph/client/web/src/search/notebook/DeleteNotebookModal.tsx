import React, { useCallback } from 'react'
import { useHistory } from 'react-router'
import { Observable } from 'rxjs'
import { mergeMap, startWith, tap, catchError } from 'rxjs/operators'

import { asError, isErrorLike } from '@sourcegraph/common'
import { LoadingSpinner, useEventObservable, Modal, Button, Alert } from '@sourcegraph/wildcard'

import { deleteNotebook as _deleteNotebook } from './backend'

interface DeleteNotebookProps {
    notebookId: string
    isOpen: boolean
    toggleDeleteModal: () => void
    deleteNotebook: typeof _deleteNotebook
}

const LOADING = 'loading' as const

export const DeleteNotebookModal: React.FunctionComponent<DeleteNotebookProps> = ({
    notebookId,
    deleteNotebook,
    isOpen,
    toggleDeleteModal,
}) => {
    const deleteLabelId = 'deleteNotebookId'
    const history = useHistory()

    const [onDelete, deleteCompletedOrError] = useEventObservable(
        useCallback(
            (click: Observable<React.MouseEvent<HTMLButtonElement>>) =>
                click.pipe(
                    mergeMap(() =>
                        deleteNotebook(notebookId).pipe(
                            tap(() => {
                                history.push('/notebooks')
                            }),
                            startWith(LOADING),
                            catchError(error => [asError(error)])
                        )
                    )
                ),
            [deleteNotebook, history, notebookId]
        )
    )

    return (
        <Modal isOpen={isOpen} position="center" onDismiss={toggleDeleteModal} aria-labelledby={deleteLabelId}>
            <h3 className="text-danger" id={deleteLabelId}>
                Delete the notebook?
            </h3>

            <p>
                <strong>This action cannot be undone.</strong>
            </p>
            {(!deleteCompletedOrError || isErrorLike(deleteCompletedOrError)) && (
                <div className="text-right">
                    <Button className="mr-2" onClick={toggleDeleteModal} variant="secondary" outline={true}>
                        Cancel
                    </Button>
                    <Button onClick={onDelete} variant="danger">
                        Yes, delete the notebook
                    </Button>
                    {isErrorLike(deleteCompletedOrError) && (
                        <Alert className="mt-2" variant="danger">
                            Error deleting notebook: {deleteCompletedOrError.message}
                        </Alert>
                    )}
                </div>
            )}
            {deleteCompletedOrError && <div>{deleteCompletedOrError === 'loading' && <LoadingSpinner />}</div>}
        </Modal>
    )
}
