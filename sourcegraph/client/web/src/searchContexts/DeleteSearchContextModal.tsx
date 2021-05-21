import Dialog from '@reach/dialog'
import React, { useCallback } from 'react'
import { useHistory } from 'react-router'
import { Observable } from 'rxjs'
import { mergeMap, startWith, tap, catchError } from 'rxjs/operators'

import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { ISearchContext } from '@sourcegraph/shared/src/graphql/schema'
import { asError, isErrorLike } from '@sourcegraph/shared/src/util/errors'
import { useEventObservable } from '@sourcegraph/shared/src/util/useObservable'

import { ALLOW_NAVIGATION } from '../components/AwayPrompt'
import { SearchContextProps } from '../search'

interface DeleteSearchContextModalProps extends Pick<SearchContextProps, 'deleteSearchContext'> {
    isOpen: boolean
    searchContext: ISearchContext
    toggleDeleteModal: () => void
}

export const DeleteSearchContextModal: React.FunctionComponent<DeleteSearchContextModalProps> = ({
    isOpen,
    deleteSearchContext,
    toggleDeleteModal,
    searchContext,
}) => {
    const LOADING = 'loading' as const
    const deleteLabelId = 'deleteSearchContextId'
    const history = useHistory()

    const [onDelete, deleteCompletedOrError] = useEventObservable(
        useCallback(
            (click: Observable<React.MouseEvent<HTMLButtonElement>>) =>
                click.pipe(
                    mergeMap(() =>
                        deleteSearchContext(searchContext.id).pipe(
                            tap(() => {
                                history.push('/contexts', ALLOW_NAVIGATION)
                            }),
                            startWith(LOADING),
                            catchError(error => [asError(error)])
                        )
                    )
                ),
            [deleteSearchContext, history, searchContext]
        )
    )

    return (
        <Dialog
            isOpen={isOpen}
            className="modal-body modal-body--centered p-4 rounded border"
            onDismiss={toggleDeleteModal}
            aria-labelledby={deleteLabelId}
            data-testid="delete-search-context-modal"
        >
            <h3 className="text-danger" id={deleteLabelId}>
                Delete search context?
            </h3>

            <p>
                <strong>This action cannot be undone.</strong>
            </p>
            {(!deleteCompletedOrError || isErrorLike(deleteCompletedOrError)) && (
                <div className="text-right">
                    <button type="button" className="btn btn-outline-secondary mr-2" onClick={toggleDeleteModal}>
                        Cancel
                    </button>
                    <button
                        type="button"
                        className="btn btn-danger"
                        data-testid="confirm-delete-search-context"
                        onClick={onDelete}
                    >
                        Yes, delete search context
                    </button>
                    {isErrorLike(deleteCompletedOrError) && (
                        <div className="alert-danger">
                            Error deleting search context: {deleteCompletedOrError.message}
                        </div>
                    )}
                </div>
            )}
            {deleteCompletedOrError && (
                <div>{deleteCompletedOrError === 'loading' && <LoadingSpinner className="icon-inline" />}</div>
            )}
        </Dialog>
    )
}
