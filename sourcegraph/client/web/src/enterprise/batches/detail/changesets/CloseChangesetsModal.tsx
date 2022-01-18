import React, { useCallback, useState } from 'react'

import { asError, isErrorLike } from '@sourcegraph/common'
import { Button, LoadingSpinner, Modal } from '@sourcegraph/wildcard'

import { ErrorAlert } from '../../../../components/alerts'
import { Scalars } from '../../../../graphql-operations'
import { closeChangesets as _closeChangesets } from '../backend'

export interface CloseChangesetsModalProps {
    onCancel: () => void
    afterCreate: () => void
    batchChangeID: Scalars['ID']
    changesetIDs: Scalars['ID'][]

    /** For testing only. */
    closeChangesets?: typeof _closeChangesets
}

export const CloseChangesetsModal: React.FunctionComponent<CloseChangesetsModalProps> = ({
    onCancel,
    afterCreate,
    batchChangeID,
    changesetIDs,
    closeChangesets = _closeChangesets,
}) => {
    const [isLoading, setIsLoading] = useState<boolean | Error>(false)

    const onSubmit = useCallback<React.FormEventHandler>(async () => {
        setIsLoading(true)
        try {
            await closeChangesets(batchChangeID, changesetIDs)
            afterCreate()
        } catch (error) {
            setIsLoading(asError(error))
        }
    }, [changesetIDs, closeChangesets, batchChangeID, afterCreate])

    return (
        <Modal onDismiss={onCancel} aria-labelledby={MODAL_LABEL_ID}>
            <h3 id={MODAL_LABEL_ID}>Close changesets</h3>
            <p className="mb-4">Are you sure you want to close all the selected changesets on the code hosts?</p>
            {isErrorLike(isLoading) && <ErrorAlert error={isLoading} />}
            <div className="d-flex justify-content-end">
                <Button
                    disabled={isLoading === true}
                    className="mr-2"
                    onClick={onCancel}
                    outline={true}
                    variant="secondary"
                >
                    Cancel
                </Button>
                <Button onClick={onSubmit} disabled={isLoading === true} variant="primary">
                    {isLoading === true && <LoadingSpinner />}
                    Close
                </Button>
            </div>
        </Modal>
    )
}

const MODAL_LABEL_ID = 'close-changesets-modal-title'
