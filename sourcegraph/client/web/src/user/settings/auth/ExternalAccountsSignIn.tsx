import React from 'react'

import classNames from 'classnames'
import type { AuthProvider } from 'src/jscontext'

import type { ErrorLike } from '@sourcegraph/common'

import { defaultExternalAccounts } from '../../../components/externalAccounts/externalAccounts'

import { ExternalAccount } from './ExternalAccount'
import type { AccountsByServiceID, UserExternalAccount } from './UserSettingsSecurityPage'

import styles from './ExternalAccountsSignIn.module.scss'

export interface NormalizedExternalAccount {
    name: string
    icon: React.ComponentType<React.PropsWithChildren<{ className?: string }>>
    // some data may be missing if account is not setup
    external?: UserExternalAccount['publicAccountData'] & {
        id: string
    }
}

interface Props {
    accounts: AccountsByServiceID
    authProviders: AuthProvider[]
    onDidRemove: (id: string, name: string) => void
    onDidError: (error: ErrorLike) => void
    onDidAdd: () => void
}

const getNormalizedAccount = (
    accounts: Partial<Record<string, UserExternalAccount[]>>,
    authProvider: AuthProvider
): NormalizedExternalAccount | null => {
    if (
        authProvider.serviceType === 'builtin' ||
        authProvider.serviceType === 'http-header' ||
        authProvider.serviceType === 'sourcegraph-operator'
    ) {
        return null
    }

    const { icon, title: name } = defaultExternalAccounts[authProvider.serviceType]

    const normalizedAccount: NormalizedExternalAccount = {
        icon,
        name,
    }

    const providerAccounts = accounts[authProvider.serviceID]

    const providerAccount = providerAccounts?.find(acc => acc.clientID === authProvider.clientID)
    if (providerAccount?.publicAccountData) {
        normalizedAccount.external = {
            id: providerAccount.id,
            ...providerAccount.publicAccountData,
        }
    }

    return normalizedAccount
}

export const ExternalAccountsSignIn: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    accounts,
    authProviders,
    onDidRemove,
    onDidError,
    onDidAdd,
}) => (
    <>
        {authProviders && (
            <ul className="list-group">
                {authProviders.map(authProvider => {
                    // if auth provider for this account doesn't exist -
                    // don't display the account as an option
                    const normAccount = getNormalizedAccount(accounts, authProvider)
                    if (normAccount) {
                        return (
                            <li
                                key={normAccount.external ? normAccount.external.id : authProvider.serviceID}
                                className={classNames('list-group-item', styles.externalAccount)}
                            >
                                <ExternalAccount
                                    account={normAccount}
                                    authProvider={authProvider}
                                    onDidRemove={onDidRemove}
                                    onDidError={onDidError}
                                    onDidAdd={onDidAdd}
                                />
                            </li>
                        )
                    }

                    return null
                })}
            </ul>
        )}
    </>
)
