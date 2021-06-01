import React, { FunctionComponent, useEffect, useState, useCallback } from 'react'

import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { gql, dataOrThrowErrors } from '@sourcegraph/shared/src/graphql/graphql'
import { asError, ErrorLike, isErrorLike } from '@sourcegraph/shared/src/util/errors'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'
import { Container, PageHeader } from '@sourcegraph/wildcard'

import { requestGraphQL } from '../../../backend/graphql'
import { ErrorAlert } from '../../../components/alerts'
import { PageTitle } from '../../../components/PageTitle'
import { Scalars, UserAreaUserFields, UserEmailsResult, UserEmailsVariables } from '../../../graphql-operations'
import { siteFlags } from '../../../site/backend'
import { eventLogger } from '../../../tracking/eventLogger'

import { AddUserEmailForm } from './AddUserEmailForm'
import { SetUserPrimaryEmailForm } from './SetUserPrimaryEmailForm'
import { UserEmail } from './UserEmail'

interface Props {
    user: UserAreaUserFields
}

type UserEmail = NonNullable<UserEmailsResult['node']>['emails'][number]
type Status = undefined | 'loading' | 'loaded' | ErrorLike
type EmailActionError = undefined | ErrorLike

export const UserSettingsEmailsPage: FunctionComponent<Props> = ({ user }) => {
    const [emails, setEmails] = useState<UserEmail[]>([])
    const [statusOrError, setStatusOrError] = useState<Status>()
    const [emailActionError, setEmailActionError] = useState<EmailActionError>()

    const onEmailRemove = useCallback(
        (deletedEmail: string): void => {
            setEmails(emails => emails.filter(({ email }) => email !== deletedEmail))
            // always cleanup email action errors when removing emails
            setEmailActionError(undefined)
        },
        [setEmailActionError]
    )

    const fetchEmails = useCallback(async (): Promise<void> => {
        setStatusOrError('loading')

        const fetchedEmails = await fetchUserEmails(user.id)

        // always cleanup email action errors when re-fetching emails
        setEmailActionError(undefined)

        if (fetchedEmails?.node?.emails) {
            setEmails(fetchedEmails.node.emails)
            setStatusOrError('loaded')
        } else {
            setStatusOrError(asError("Sorry, we couldn't fetch user emails. Try again?"))
        }
    }, [user, setStatusOrError, setEmails])

    const flags = useObservable(siteFlags)

    useEffect(() => {
        eventLogger.logViewEvent('UserSettingsEmails')
    }, [])

    useEffect(() => {
        fetchEmails().catch(error => {
            setStatusOrError(asError(error))
        })
    }, [fetchEmails])

    if (statusOrError === 'loading') {
        return <LoadingSpinner className="icon-inline" />
    }

    if (isErrorLike(statusOrError)) {
        return <ErrorAlert className="mt-2" error={statusOrError} />
    }

    return (
        <div className="user-settings-emails-page">
            <PageTitle title="Emails" />
            <PageHeader headingElement="h2" path={[{ text: 'Emails' }]} className="mb-3" />

            {flags && !flags.sendsEmailVerificationEmails && (
                <div className="alert alert-warning">
                    Sourcegraph is not configured to send email verifications. Newly added email addresses must be
                    manually verified by a site admin.
                </div>
            )}

            {isErrorLike(emailActionError) && <ErrorAlert className="mt-2" error={emailActionError} />}

            <Container>
                <h3>All configured emails</h3>
                <ul className="list-group">
                    {emails.map(email => (
                        <li key={email.email} className="user-settings-emails-page__list-item list-group-item">
                            <UserEmail
                                user={user.id}
                                email={email}
                                onEmailVerify={fetchEmails}
                                onEmailResendVerification={fetchEmails}
                                onDidRemove={onEmailRemove}
                                onError={setEmailActionError}
                            />
                        </li>
                    ))}
                    {emails.length === 0 && (
                        <li className="user-settings-emails-page__list-item list-group-item text-muted">No emails</li>
                    )}
                </ul>
                {/* re-fetch emails on onDidAdd to guarantee correct state */}
                <AddUserEmailForm
                    className="user-settings-emails-page__email-form"
                    user={user.id}
                    onDidAdd={fetchEmails}
                />
                <hr className="my-4" />
                <SetUserPrimaryEmailForm user={user.id} emails={emails} onDidSet={fetchEmails} />
            </Container>
        </div>
    )
}

async function fetchUserEmails(userID: Scalars['ID']): Promise<UserEmailsResult> {
    return dataOrThrowErrors(
        await requestGraphQL<UserEmailsResult, UserEmailsVariables>(
            gql`
                query UserEmails($user: ID!) {
                    node(id: $user) {
                        ... on User {
                            emails {
                                email
                                isPrimary
                                verified
                                verificationPending
                                viewerCanManuallyVerify
                            }
                        }
                    }
                }
            `,
            { user: userID }
        ).toPromise()
    )
}
