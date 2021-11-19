import Dialog from '@reach/dialog'
import React, { useState, useCallback } from 'react'

import { Link } from '@sourcegraph/shared/src/components/Link'

import { Form } from '../../../../../branded/src/components/Form'
import { asError, ErrorLike } from '../../../../../shared/src/util/errors'
import { updateExternalService } from '../../../components/externalServices/backend'
import { LoaderButton } from '../../../components/LoaderButton'
import { Scalars, ExternalServiceKind, ListExternalServiceFields } from '../../../graphql-operations'

import { EncryptedDataIcon } from './components/EncryptedDataIcon'
import { getMachineUserFragment } from './modalHints'

interface CodeHostConfig {
    url: string
    token: string
}

const updateConfigToken = (config: string, token: string): string => {
    const updatedConfig = JSON.parse(config) as CodeHostConfig
    updatedConfig.token = token
    return JSON.stringify(updatedConfig, null, 2)
}

export const UpdateCodeHostConnectionModal: React.FunctionComponent<{
    serviceID: Scalars['ID']
    serviceConfig: string
    serviceName: string
    orgName: string
    kind: ExternalServiceKind
    onDidUpdate: (service: ListExternalServiceFields) => void
    onDidCancel: () => void
    onDidError: (error: ErrorLike) => void

    hintFragment?: React.ReactFragment
}> = ({ serviceID, serviceConfig, serviceName, hintFragment, onDidUpdate, onDidCancel, onDidError }) => {
    const [token, setToken] = useState<string>('')
    const [isLoading, setIsLoading] = useState(false)
    const [didAckMachineUserHint, setAckMachineUserHint] = useState(false)

    const onChangeToken: React.ChangeEventHandler<HTMLInputElement> = event => setToken(event.target.value)
    const machineUserFragment = getMachineUserFragment(serviceName)

    const handleError = useCallback(
        (error: ErrorLike | string): void => {
            setIsLoading(false)
            onDidCancel()
            onDidError(asError(error))
        },
        [onDidCancel, onDidError]
    )

    const onTokenSubmit = useCallback<React.FormEventHandler<HTMLFormElement>>(
        async event => {
            event.preventDefault()
            setIsLoading(true)

            try {
                if (token) {
                    const config = updateConfigToken(serviceConfig, token)

                    const { webhookURL, ...newService } = await updateExternalService({
                        input: { id: serviceID, config },
                    })

                    onDidUpdate(newService)
                    onDidCancel()
                }
            } catch (error) {
                handleError(error)
            }
        },
        [serviceConfig, serviceID, token, onDidCancel, handleError, onDidUpdate]
    )

    return (
        <Dialog
            className="modal-body modal-body--top-third p-4 rounded border"
            aria-labelledby={`heading--update-${serviceName}-code-host`}
            onDismiss={onDidCancel}
        >
            <div className="web-content">
                <h3 id={`heading--update-${serviceName}-code-host`} className="mb-4">
                    Update {serviceName} connection
                </h3>
                <Form onSubmit={onTokenSubmit}>
                    <div className="form-group mb-4">
                        <div className="alert alert-info" role="alert">
                            Updating the access token may affect which repositories can be synced with Sourcegraph.{' '}
                            <Link
                                to="https://docs.sourcegraph.com/cloud/access_tokens_on_cloud"
                                target="_blank"
                                rel="noopener noreferrer"
                                className="font-weight-normal"
                            >
                                Learn more
                            </Link>
                            .
                        </div>
                        {didAckMachineUserHint ? (
                            <>
                                {' '}
                                <label htmlFor="code-host-token">Access token</label>
                                <div className="position-relative">
                                    <input
                                        id="code-host-token"
                                        name="code-host-token"
                                        type="text"
                                        value={token}
                                        onChange={onChangeToken}
                                        className="form-control pr-4"
                                        autoComplete="off"
                                    />
                                    <EncryptedDataIcon />
                                </div>
                                <p className="mt-1">{hintFragment}</p>
                            </>
                        ) : (
                            machineUserFragment
                        )}
                    </div>
                    <div className="d-flex justify-content-end">
                        <button type="button" className="btn btn-outline-secondary mr-2" onClick={onDidCancel}>
                            Cancel
                        </button>

                        {didAckMachineUserHint ? (
                            <LoaderButton
                                type="submit"
                                className="btn btn-primary"
                                loading={isLoading}
                                disabled={!token || isLoading}
                                label="Update code host connection"
                                alwaysShowLabel={true}
                            />
                        ) : (
                            <button
                                type="button"
                                className="btn btn-secondary mr-2"
                                onClick={() => setAckMachineUserHint(previousAckStatus => !previousAckStatus)}
                            >
                                I understand, continue
                            </button>
                        )}
                    </div>
                </Form>
            </div>
        </Dialog>
    )
}
