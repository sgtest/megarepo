import classNames from 'classnames'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import CheckCircleIcon from 'mdi-react/CheckCircleIcon'
import FileUploadIcon from 'mdi-react/FileUploadIcon'
import TimerSandIcon from 'mdi-react/TimerSandIcon'
import React, { FunctionComponent } from 'react'

import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'

import { LSIFIndexState, LSIFUploadState } from '../../../../graphql-operations'

export interface CodeIntelStateIconProps {
    state: LSIFUploadState | LSIFIndexState
    className?: string
}

export const CodeIntelStateIcon: FunctionComponent<CodeIntelStateIconProps> = ({ state, className }) =>
    state === LSIFUploadState.UPLOADING ? (
        <FileUploadIcon className={className} />
    ) : state === LSIFUploadState.DELETING ? (
        <CheckCircleIcon className={classNames('text-muted', className)} />
    ) : state === LSIFUploadState.QUEUED || state === LSIFIndexState.QUEUED ? (
        <TimerSandIcon className={className} />
    ) : state === LSIFUploadState.PROCESSING || state === LSIFIndexState.PROCESSING ? (
        <LoadingSpinner className={className} />
    ) : state === LSIFUploadState.COMPLETED || state === LSIFIndexState.COMPLETED ? (
        <CheckCircleIcon className={classNames('text-success', className)} />
    ) : state === LSIFUploadState.ERRORED || state === LSIFIndexState.ERRORED ? (
        <AlertCircleIcon className={classNames('text-danger', className)} />
    ) : (
        <></>
    )
