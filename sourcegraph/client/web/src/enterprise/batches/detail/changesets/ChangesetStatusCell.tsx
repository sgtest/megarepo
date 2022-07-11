import React from 'react'

import classNames from 'classnames'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import ArchiveIcon from 'mdi-react/ArchiveIcon'
import AutorenewIcon from 'mdi-react/AutorenewIcon'
import DeleteIcon from 'mdi-react/DeleteIcon'
import LockIcon from 'mdi-react/LockIcon'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'
import SourceMergeIcon from 'mdi-react/SourceMergeIcon'
import SourcePullIcon from 'mdi-react/SourcePullIcon'
import TimerSandIcon from 'mdi-react/TimerSandIcon'

import { Tooltip } from '@sourcegraph/wildcard'

import { ChangesetFields, ChangesetState, Scalars } from '../../../../graphql-operations'

import { ChangesetStatusScheduled } from './ChangesetStatusScheduled'

export interface ChangesetStatusCellProps {
    className?: string
    id?: Scalars['ID']
    state: ChangesetFields['state']
}

export const ChangesetStatusCell: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusCellProps>> = ({
    id,
    state,
    className = 'd-flex',
}) => {
    switch (state) {
        case ChangesetState.FAILED:
            return <ChangesetStatusError className={className} />
        case ChangesetState.RETRYING:
            return <ChangesetStatusRetrying className={className} />
        case ChangesetState.SCHEDULED:
            return <ChangesetStatusScheduled className={className} id={id} />
        case ChangesetState.PROCESSING:
            return <ChangesetStatusProcessing className={className} />
        case ChangesetState.UNPUBLISHED:
            return <ChangesetStatusUnpublished className={className} />
        case ChangesetState.OPEN:
            return <ChangesetStatusOpen className={className} />
        case ChangesetState.DRAFT:
            return <ChangesetStatusDraft className={className} />
        case ChangesetState.CLOSED:
            return <ChangesetStatusClosed className={className} />
        case ChangesetState.MERGED:
            return <ChangesetStatusMerged className={className} />
        case ChangesetState.READONLY:
            return <ChangesetStatusReadOnly className={className} />
        case ChangesetState.DELETED:
            return <ChangesetStatusDeleted className={className} />
    }
}

const iconClassNames = 'm-0 text-nowrap flex-column align-items-center justify-content-center'

interface ChangesetStatusIconProps extends React.HTMLAttributes<HTMLDivElement> {
    label?: React.ReactNode
    className?: string
}

export const ChangesetStatusUnpublished: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Unpublished</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <SourceBranchIcon role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusClosed: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Closed</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <SourcePullIcon className="text-danger" role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusMerged: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Merged</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <SourceMergeIcon className="text-merged" role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusOpen: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Open</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <SourcePullIcon className="text-success" role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusDraft: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Draft</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <SourcePullIcon role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusDeleted: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Deleted</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <DeleteIcon role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusError: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span className="text-danger">Failed</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <AlertCircleIcon className="text-danger" role="presentation" />
        {label}
    </div>
)
export const ChangesetStatusRetrying: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Retrying</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <AutorenewIcon role="presentation" />
        {label}
    </div>
)

export const ChangesetStatusProcessing: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Processing</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <TimerSandIcon role="presentation" />
        {label}
    </div>
)

export const ChangesetStatusArchived: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Archived</span>,
    className,
    ...props
}) => (
    <div className={classNames(iconClassNames, className)} {...props}>
        <ArchiveIcon role="presentation" />
        {label}
    </div>
)

export const ChangesetStatusReadOnly: React.FunctionComponent<React.PropsWithChildren<ChangesetStatusIconProps>> = ({
    label = <span>Read-only</span>,
    className,
    ...props
}) => (
    <Tooltip content="This changeset is read-only, and cannot be modified. This is usually caused by the repository being archived.">
        <div className={classNames(iconClassNames, className)} {...props}>
            <LockIcon role="presentation" />
            {label}
        </div>
    </Tooltip>
)
