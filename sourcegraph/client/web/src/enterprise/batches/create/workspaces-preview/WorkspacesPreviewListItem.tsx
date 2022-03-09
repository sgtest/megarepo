import classNames from 'classnames'
import CloseIcon from 'mdi-react/CloseIcon'
import ContentSaveIcon from 'mdi-react/ContentSaveIcon'
import DeleteIcon from 'mdi-react/DeleteIcon'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'
import React, { useCallback, useState } from 'react'

import { Button, Icon } from '@sourcegraph/wildcard'

import { PreviewBatchSpecWorkspaceFields } from '../../../../graphql-operations'

import styles from './WorkspacesPreviewListItem.module.scss'

interface WorkspacesPreviewListItemProps {
    item: PreviewBatchSpecWorkspaceFields
    /** Whether or not this item is stale */
    isStale: boolean
    /** Function to automatically update batch spec to exclude this item. */
    exclude: (repo: string, branch: string) => void
    /** Whether this item should take the lighter or darker variant of background color. */
    variant: 'light' | 'dark'
}

export const WorkspacesPreviewListItem: React.FunctionComponent<WorkspacesPreviewListItemProps> = ({
    item,
    isStale,
    exclude,
    variant,
}) => {
    const [toBeExcluded, setToBeExcluded] = useState(false)

    // TODO: https://github.com/sourcegraph/sourcegraph/issues/25085
    const handleExclude = useCallback(() => {
        setToBeExcluded(true)
        exclude(item.repository.name, item.branch.displayName)
    }, [exclude, item])

    return (
        <li
            className={classNames(
                'd-flex align-items-center px-2 py-3 w-100',
                variant === 'light' ? styles.light : styles.dark
            )}
            key={`${item.repository.id}_${item.branch.target.oid}_${item.path || '/'}`}
        >
            <div className={classNames(styles.statusContainer, 'mr-2')}>
                <StatusIcon status={toBeExcluded ? 'to-exclude' : item.cachedResultFound ? 'cached' : 'none'} />
            </div>
            <div className="flex-1">
                <h4 className={classNames(styles.overflow, (toBeExcluded || isStale) && styles.stale)}>
                    {item.repository.name}
                </h4>
                {item.path !== '' && item.path !== '/' ? (
                    <span className={classNames(styles.overflow, 'd-block text-muted')}>{item.path}</span>
                ) : null}
                <div className="d-flex align-items-center text-muted text-monospace mt-1">
                    <Icon className="mr-1" as={SourceBranchIcon} />
                    <small>{item.branch.displayName}</small>
                </div>
            </div>
            <Button
                className="p-0 my-0 mx-2"
                disabled={toBeExcluded}
                data-tooltip={toBeExcluded ? undefined : 'Omit this repository from batch spec file'}
                onClick={handleExclude}
            >
                <Icon as={CloseIcon} />
            </Button>
        </li>
    )
}

type StatusIconStatus = 'none' | 'cached' | 'to-exclude'

const StatusIcon: React.FunctionComponent<{ status: StatusIconStatus }> = ({ status }) => {
    switch (status) {
        case 'none':
            return null
        case 'cached':
            return <Icon data-tooltip="A cached result was found for this workspace." as={ContentSaveIcon} />
        case 'to-exclude':
            return (
                <Icon
                    data-tooltip="Your batch spec was modified to exclude this workspace. Preview again to update."
                    as={DeleteIcon}
                />
            )
    }
}
