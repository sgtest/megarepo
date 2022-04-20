import React, { useCallback, useState } from 'react'

import classNames from 'classnames'
import CheckboxBlankCircleOutlineIcon from 'mdi-react/CheckboxBlankCircleOutlineIcon'
import CheckCircleOutlineIcon from 'mdi-react/CheckCircleOutlineIcon'

import { Badge, Button, Icon } from '@sourcegraph/wildcard'

import { defaultExternalServices } from '../../../components/externalServices/externalServices'
import { BatchChangesCodeHostFields, Scalars } from '../../../graphql-operations'

import { AddCredentialModal } from './AddCredentialModal'
import { RemoveCredentialModal } from './RemoveCredentialModal'
import { ViewCredentialModal } from './ViewCredentialModal'

import styles from './CodeHostConnectionNode.module.scss'

export interface CodeHostConnectionNodeProps {
    node: BatchChangesCodeHostFields
    refetchAll: () => void
    userID: Scalars['ID'] | null
}

type OpenModal = 'add' | 'view' | 'delete'

export const CodeHostConnectionNode: React.FunctionComponent<CodeHostConnectionNodeProps> = ({
    node,
    refetchAll,
    userID,
}) => {
    const ExternalServiceIcon = defaultExternalServices[node.externalServiceKind].icon

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
        refetchAll()
    }, [refetchAll])

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
                            <Icon
                                className="text-success test-code-host-connection-node-enabled"
                                data-tooltip="Connected"
                                as={CheckCircleOutlineIcon}
                            />
                        )}
                        {!isEnabled && (
                            <Icon
                                className="text-danger test-code-host-connection-node-disabled"
                                data-tooltip="No token set"
                                as={CheckboxBlankCircleOutlineIcon}
                            />
                        )}
                        <Icon className="mx-2" as={ExternalServiceIcon} /> {node.externalServiceURL}{' '}
                        {!isEnabled && node.credential?.isSiteCredential && (
                            <Badge
                                variant="secondary"
                                tooltip="Changesets on this code host will
                            be created with a global token until a personal access token is added."
                            >
                                Global token
                            </Badge>
                        )}
                    </h3>
                    <div className="mb-0 d-flex justify-content-end flex-grow-1">
                        {isEnabled && (
                            <>
                                <Button
                                    className="text-danger text-nowrap test-code-host-connection-node-btn-remove"
                                    onClick={onClickRemove}
                                    variant="link"
                                >
                                    Remove
                                </Button>
                                {node.requiresSSH && (
                                    <Button onClick={onClickView} className="text-nowrap ml-2" variant="secondary">
                                        View public key
                                    </Button>
                                )}
                            </>
                        )}
                        {!isEnabled && (
                            /*
                                a11y-ignore
                                Rule: "color-contrast" (Elements must have sufficient color contrast)
                                GitHub issue: https://github.com/sourcegraph/sourcegraph/issues/33343
                            */
                            <Button
                                className="a11y-ignore text-nowrap test-code-host-connection-node-btn-add"
                                onClick={onClickAdd}
                                variant="success"
                            >
                                Add credentials
                            </Button>
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
                    requiresUsername={node.requiresUsername}
                />
            )}
        </>
    )
}
