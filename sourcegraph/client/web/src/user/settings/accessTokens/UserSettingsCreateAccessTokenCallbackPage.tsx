import React, { useEffect, useMemo, useState } from 'react'

import { RouteComponentProps } from 'react-router'
import { NEVER } from 'rxjs'
import { catchError, startWith, tap } from 'rxjs/operators'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { Form } from '@sourcegraph/branded/src/components/Form'
import { asError, isErrorLike } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Container, PageHeader, Button, useObservable, Link, LoadingSpinner, Alert, Text } from '@sourcegraph/wildcard'

import { AccessTokenScopes } from '../../../auth/accessToken'
import { CopyableText } from '../../../components/CopyableText'
import { PageTitle } from '../../../components/PageTitle'
import { CreateAccessTokenResult } from '../../../graphql-operations'
import { UserSettingsAreaRouteContext } from '../UserSettingsArea'

import { createAccessToken } from './create'

interface Props
    extends Pick<UserSettingsAreaRouteContext, 'authenticatedUser' | 'user'>,
        Pick<RouteComponentProps<{}>, 'history' | 'match'>,
        TelemetryProps {
    /**
     * Called when a new access token is created and should be temporarily displayed to the user.
     */
    onDidCreateAccessToken: (value: CreateAccessTokenResult['createAccessToken']) => void
}
interface TokenRequester {
    /** The name of the source */
    name: string
    /** The URL where the token should be added to */
    /** SECURITY: Local context only! Do not send token to non-local servers*/
    redirectURL: string
    /** A description of where the request is coming from */
    description: string
    /** The message to show users when the token has been created successfully */
    successMessage?: string
    /** The message to show users in case the token cannot be imported automatically */
    infoMessage?: string
    /** How the redirect URL should be open: open in same tab vs open in a new-tab */
    /** Default: Open link in same tab */
    callbackType?: 'open' | 'new-tab'
    /** Show button to redirect URL on click */
    showRedirectButton?: boolean
}
// SECURITY: Only accept callback requests from requesters on this allowed list
const REQUESTERS: Record<string, TokenRequester> = {
    VSCEAUTH: {
        name: 'VS Code Extension',
        redirectURL: 'vscode://sourcegraph.sourcegraph?code=$TOKEN',
        description: 'Auth from VS Code Extension for Sourcegraph',
        successMessage:
            'Importing your token will automatically connect your Sourcegraph account in the VS Code extension.',
        infoMessage:
            'Please make sure you have VS Code running on your machine if you do not see an open dialog in your browser.',
        callbackType: 'new-tab',
        showRedirectButton: true,
    },
}
/**
 * This page acts as a callback URL after the authentication process has been completed by a user.
 * This can be shared among different SG integrations as long as the value that is being passed in
 * using the 'requestFrom' param (.../user/settings/tokens/new/callback?requestFrom=$SOURCE) is included in
 * the REQUESTERS allow list above.
 * Once the request has been validated, the user will then be redirected back to the source with the newly created token passing
 * in as a new URL param, using the redirect URL associated with the allowlisted requester The token should then be processed by the extension's
 * URL handler (For example, "vscode://sourcegraph/sourcegraph?code=$TOKEN" for the VS Code extension)
 */
export const UserSettingsCreateAccessTokenCallbackPage: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    telemetryService,
    onDidCreateAccessToken,
    user,
    history,
    match,
}) => {
    useMemo(() => {
        telemetryService.logPageView('NewAccessTokenCallback')
    }, [telemetryService])
    /** Get the requester from the url parameters if any */
    const requestFrom = new URLSearchParams(history.location.search).get('requestFrom')
    /** The validated requester where the callback request originally comes from. */
    const [requester, setRequester] = useState<TokenRequester | null | undefined>(undefined)
    /** The contents of the note input field. */
    const [note, setNote] = useState<string>('')
    /** The newly created token if any. */
    const [newToken, setNewToken] = useState('')
    // Check and Match URL Search Prams
    useEffect((): void => {
        // SECURITY: Verify if the request is coming from an allowlisted source
        const isRequestValid = requestFrom && requestFrom in REQUESTERS
        if (requestFrom && requester === undefined) {
            setRequester(isRequestValid ? REQUESTERS[requestFrom] : null)
            setNote(isRequestValid ? REQUESTERS[requestFrom].name : '')
        }
        // Redirect users back to tokens page if none or invalid url params provided
        if (!requestFrom || (!requester && requester !== undefined)) {
            history.push(`${match.url.replace(/\/new\/callback$/, '')}`)
        }
    }, [history, match.url, requestFrom, requester])
    /**
     * We use this to handle token creation request from redirections.
     * Don't create token if this page wasn't linked to from a valid
     * requester (e.g. VS Code extension).
     */
    const creationOrError = useObservable(
        useMemo(
            () =>
                (requester ? createAccessToken(user.id, [AccessTokenScopes.UserAll], note) : NEVER).pipe(
                    tap(result => {
                        // SECURITY: If the request was from a valid requestor, redirect to the allowlisted redirect URL.
                        // SECURITY: Local context ONLY
                        if (requester) {
                            onDidCreateAccessToken(result)
                            setNewToken(result.token)
                            const uri = requester?.redirectURL.replace('$TOKEN', result.token)
                            switch (requester.callbackType) {
                                case 'new-tab':
                                    window.open(uri, '_blank')
                                default:
                                    // open the redirect link in the same tab
                                    window.location.replace(uri)
                            }
                        }
                    }),
                    startWith('loading'),
                    catchError(error => [asError(error)])
                ),
            [requester, user.id, note, onDidCreateAccessToken]
        )
    )
    /**
     * If there's a uriPattern but no result or error yet, we can assume that creation is
     * in progress and show a loading spinner + message.
     */
    if (creationOrError === 'loading') {
        return <LoadingSpinner />
    }
    if (!requester) {
        return null
    }
    return (
        <div className="user-settings-create-access-token-page">
            <PageTitle title="Create access token" />
            <PageHeader
                path={[{ text: `Connect my account to ${requester ? requester.name : ''}` }]}
                headingElement="h2"
                className="mb-3"
            />
            {!isErrorLike(creationOrError) && requester?.infoMessage && (
                <Alert className="my-2" variant="warning">
                    <Text className="my-2">{requester?.infoMessage}</Text>
                </Alert>
            )}
            {newToken && requester && (
                <Form>
                    <Container className="mb-3">
                        <Alert className="access-token-created-alert mt-3" variant="success">
                            <Text weight="bold">{requester.name} access token successfully generated</Text>
                            <Text>{requester?.successMessage}</Text>
                            <CopyableText className="test-access-token" text={newToken} size={48} />
                            <Text className="form-help text-muted" size="small">
                                This is a one-time access token to connect your account to {requester.name}. You will
                                not be able to see this token again once the window is closed.
                            </Text>
                        </Alert>
                    </Container>
                    <div className="mb-3">
                        {requester.showRedirectButton && (
                            <Button
                                className="mr-2"
                                to={requester.redirectURL.replace('$TOKEN', newToken)}
                                disabled={creationOrError === 'loading'}
                                variant="primary"
                                as={Link}
                            >
                                Import token to {requester.name}
                            </Button>
                        )}
                        <Button
                            to={match.url.replace(/\/new\/callback$/, '')}
                            disabled={creationOrError === 'loading'}
                            variant="secondary"
                            as={Link}
                        >
                            Back
                        </Button>
                    </div>
                </Form>
            )}
            {isErrorLike(creationOrError) && <ErrorAlert className="my-3" error={creationOrError} />}
        </div>
    )
}
