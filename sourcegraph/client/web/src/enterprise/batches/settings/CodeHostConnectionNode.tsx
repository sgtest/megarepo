import classNames from 'classnames'
import CheckboxBlankCircleOutlineIcon from 'mdi-react/CheckboxBlankCircleOutlineIcon'
import CheckCircleOutlineIcon from 'mdi-react/CheckCircleOutlineIcon'
import React, { useCallback, useState } from 'react'
import { Subject } from 'rxjs'

import { defaultExternalServices } from '../../../components/externalServices/externalServices'
import { BatchChangesCodeHostFields, Scalars } from '../../../graphql-operations'

import { AddCredentialModal } from './AddCredentialModal'
import styles from './CodeHostConnectionNode.module.scss'
import { RemoveCredentialModal } from './RemoveCredentialModal'
import { ViewCredentialModal } from './ViewCredentialModal'

export interface CodeHostConnectionNodeProps {
    node: BatchChangesCodeHostFields
    userID: Scalars['ID'] | null
    updateList: Subject<void>
}

type OpenModal = 'add' | 'view' | 'delete'

export const CodeHostConnectionNode: React.FunctionComponent<CodeHostConnectionNodeProps> = ({
    node,
    userID,
    updateList,
}) => {
    const Icon = defaultExternalServices[node.externalServiceKind].icon

    const [openModal, setOpenModal] = useState<OpenModal | undefined>()
    const onClickAdd = useCallback(() => {
        setOpenModal('add')
    }, [])
    const onClickRemove = useCallback<React.MouseEventHandler>(event => {
        event.preventDefault()
        setOpenModal('delete')
    }, [])
    const onClickView = useCallback<React.MouseEventHandler>(event => {
        event.preventDefault()
        setOpenModal('view')
    }, [])
    const closeModal = useCallback(() => {
        setOpenModal(undefined)
    }, [])
    const afterAction = useCallback(() => {
        setOpenModal(undefined)
        updateList.next()
    }, [updateList])

    const isEnabled = node.credential !== null && (userID === null || !node.credential.isSiteCredential)

    return (
        <>
            <li
                className={classNames(
                    styles.codeHostConnectionNodeContainer,
                    'list-group-item test-code-host-connection-node'
                )}
            >
                <div
                    className={classNames(
                        styles.wrapper,
                        'd-flex justify-content-between align-items-center flex-wrap mb-0'
                    )}
                >
                    <h3 className="text-nowrap mb-0">
                        {isEnabled && (
                            <CheckCircleOutlineIcon
                                className="text-success icon-inline test-code-host-connection-node-enabled"
                                data-tooltip="Connected"
                            />
                        )}
                        {!isEnabled && (
                            <CheckboxBlankCircleOutlineIcon
                                className="text-danger icon-inline test-code-host-connection-node-disabled"
                                data-tooltip="No token set"
                            />
                        )}
                        <Icon className="icon-inline mx-2" /> {node.externalServiceURL}{' '}
                        {!isEnabled && node.credential?.isSiteCredential && (
                            <span
                                className="badge badge-secondary"
                                data-tooltip="Changesets on this code host will
                            be created with a global token until a personal access token is added."
                            >
                                Global token
                            </span>
                        )}
                    </h3>
                    <div className="mb-0 d-flex justify-content-end flex-grow-1">
                        {isEnabled && (
                            <>
                                <button
                                    type="button"
                                    className="btn btn-link text-danger text-nowrap test-code-host-connection-node-btn-remove"
                                    onClick={onClickRemove}
                                >
                                    Remove
                                </button>
                                {node.requiresSSH && (
                                    <button
                                        type="button"
                                        onClick={onClickView}
                                        className="btn btn-secondary text-nowrap ml-2"
                                    >
                                        View public key
                                    </button>
                                )}
                            </>
                        )}
                        {!isEnabled && (
                            <button
                                type="button"
                                className="btn btn-success text-nowrap test-code-host-connection-node-btn-add"
                                onClick={onClickAdd}
                            >
                                Add credentials
                            </button>
                        )}
                    </div>
                </div>
            </li>
            {openModal === 'delete' && (
                <RemoveCredentialModal
                    onCancel={closeModal}
                    afterDelete={afterAction}
                    codeHost={node}
                    credential={node.credential!}
                />
            )}
            {openModal === 'view' && (
                <ViewCredentialModal onClose={closeModal} codeHost={node} credential={node.credential!} />
            )}
            {openModal === 'add' && (
                <AddCredentialModal
                    onCancel={closeModal}
                    afterCreate={afterAction}
                    userID={userID}
                    externalServiceKind={node.externalServiceKind}
                    externalServiceURL={node.externalServiceURL}
                    requiresSSH={node.requiresSSH}
                />
            )}
        </>
    )
}
