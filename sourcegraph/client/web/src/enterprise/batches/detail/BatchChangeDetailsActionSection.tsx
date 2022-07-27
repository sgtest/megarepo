import React, { useCallback, useState } from 'react'

import { mdiInformation, mdiDelete, mdiPencil } from '@mdi/js'
import * as H from 'history'

import { isErrorLike, asError } from '@sourcegraph/common'
import { Settings } from '@sourcegraph/shared/src/schema/settings.schema'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { Button, Link, Icon, Tooltip } from '@sourcegraph/wildcard'

import { isBatchChangesExecutionEnabled } from '../../../batches'
import { Scalars } from '../../../graphql-operations'
import { eventLogger } from '../../../tracking/eventLogger'

import { deleteBatchChange as _deleteBatchChange } from './backend'

export interface BatchChangeDetailsActionSectionProps extends SettingsCascadeProps<Settings> {
    batchChangeID: Scalars['ID']
    batchChangeClosed: boolean
    batchChangeNamespaceURL: string
    batchChangeURL: string
    history: H.History

    /** For testing only. */
    deleteBatchChange?: typeof _deleteBatchChange
}

export const BatchChangeDetailsActionSection: React.FunctionComponent<
    React.PropsWithChildren<BatchChangeDetailsActionSectionProps>
> = ({
    batchChangeID,
    batchChangeClosed,
    batchChangeNamespaceURL,
    batchChangeURL,
    history,
    settingsCascade,
    deleteBatchChange = _deleteBatchChange,
}) => {
    const showEditButton = isBatchChangesExecutionEnabled(settingsCascade)

    const [isDeleting, setIsDeleting] = useState<boolean | Error>(false)
    const onDeleteBatchChange = useCallback(async () => {
        if (!confirm('Do you really want to delete this batch change?')) {
            return
        }
        setIsDeleting(true)
        try {
            await deleteBatchChange(batchChangeID)
            history.push(batchChangeNamespaceURL + '/batch-changes')
        } catch (error) {
            setIsDeleting(asError(error))
        }
    }, [batchChangeID, deleteBatchChange, history, batchChangeNamespaceURL])
    if (batchChangeClosed) {
        return (
            <Tooltip content="Deleting this batch change is a final action." placement="left">
                <Button
                    className="test-batches-delete-btn"
                    onClick={onDeleteBatchChange}
                    disabled={isDeleting === true}
                    outline={true}
                    variant="danger"
                >
                    {isErrorLike(isDeleting) && (
                        <Tooltip content={isDeleting.message} placement="left">
                            <Icon aria-label={isDeleting.message} svgPath={mdiInformation} />
                        </Tooltip>
                    )}
                    <Icon aria-hidden={true} svgPath={mdiDelete} /> Delete
                </Button>
            </Tooltip>
        )
    }
    return (
        <div className="d-flex">
            {showEditButton && (
                <Button
                    to={`${batchChangeURL}/edit`}
                    className="mr-2"
                    variant="secondary"
                    as={Link}
                    onClick={() => {
                        eventLogger.log('batch_change_details:edit:clicked')
                    }}
                >
                    <Icon aria-hidden={true} svgPath={mdiPencil} /> Edit
                </Button>
            )}
            <Tooltip content="View a preview of all changes that will happen when you close this batch change.">
                <Button
                    to={`${batchChangeURL}/close`}
                    className="test-batches-close-btn"
                    variant="danger"
                    outline={true}
                    as={Link}
                    onClick={() => {
                        eventLogger.log('batch_change_details:close:clicked')
                    }}
                >
                    <Icon aria-hidden={true} svgPath={mdiDelete} /> Close
                </Button>
            </Tooltip>
        </div>
    )
}
