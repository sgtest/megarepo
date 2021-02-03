import React, { useState, useCallback } from 'react'
import { Link } from '../../../../../shared/src/components/Link'
import { LoaderButton } from '../../../components/LoaderButton'
import { ErrorLike } from '../../../../../shared/src/util/errors'
import { RemoveExternalAccountModal } from './RemoveExternalAccountModal'
import type { NormalizedMinAccount } from './ExternalAccountsSignIn'
import type { AuthProvider } from './UserSettingsSecurityPage'

interface Props {
    account: NormalizedMinAccount
    authProvider: AuthProvider
    onDidRemove: (id: string, name: string) => void
    onDidError: (error: ErrorLike) => void
}

export const ExternalAccount: React.FunctionComponent<Props> = ({ account, authProvider, onDidRemove, onDidError }) => {
    const [isLoading, setIsLoading] = useState(false)
    const [isRemoveAccountModalOpen, setIsRemoveAccountModalOpen] = useState(false)
    const toggleRemoveAccountModal = useCallback(() => setIsRemoveAccountModalOpen(!isRemoveAccountModalOpen), [
        isRemoveAccountModalOpen,
    ])

    const navigateToAuthProvider = useCallback((): void => {
        setIsLoading(true)
        window.location.assign(`${authProvider.authenticationURL as string}&redirect=${window.location.href}`)
    }, [authProvider.authenticationURL])

    const { icon: AccountIcon } = account

    return (
        <div className="p-2 d-flex align-items-start ">
            {isRemoveAccountModalOpen && account.external && (
                <RemoveExternalAccountModal
                    id={account.external.id}
                    name={account.name}
                    onDidCancel={toggleRemoveAccountModal}
                    onDidRemove={onDidRemove}
                    onDidError={onDidError}
                />
            )}
            <div className="align-self-center">
                <AccountIcon className="mb-0 mr-2" />
            </div>
            <div className="flex-1 flex-column">
                <h3 className="m-0">{account.name}</h3>
                <div className="text-muted">
                    {account.external ? (
                        <>
                            {account.external.userName} (
                            <Link to={account.external.userUrl} target="_blank" rel="noopener noreferrer">
                                <span className="text-primary">@{account.external.userLogin}</span>
                            </Link>
                            )
                        </>
                    ) : (
                        'Not connected'
                    )}
                </div>
            </div>
            <div className="align-self-center">
                {account.external ? (
                    <button type="button" className="btn btn-link text-danger px-0" onClick={toggleRemoveAccountModal}>
                        Remove
                    </button>
                ) : (
                    <LoaderButton
                        loading={isLoading}
                        label="Add"
                        type="button"
                        className="btn btn-secondary btn-block"
                        onClick={navigateToAuthProvider}
                    />
                )}
            </div>
        </div>
    )
}
