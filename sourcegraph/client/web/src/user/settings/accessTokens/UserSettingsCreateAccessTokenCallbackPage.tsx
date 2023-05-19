import React, { useCallback, useEffect, useState } from 'react'

import { useLocation, useNavigate } from 'react-router-dom'
import { NEVER, Observable } from 'rxjs'
import { catchError, startWith, switchMap, tap } from 'rxjs/operators'

import { asError, isErrorLike } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { useIsLightTheme } from '@sourcegraph/shared/src/theme'
import { Button, Link, Text, ErrorAlert, Card, H1, H2, useEventObservable } from '@sourcegraph/wildcard'

import { AccessTokenScopes } from '../../../auth/accessToken'
import { BrandLogo } from '../../../components/branding/BrandLogo'
import { CopyableText } from '../../../components/CopyableText'
import { LoaderButton } from '../../../components/LoaderButton'
import { CreateAccessTokenResult } from '../../../graphql-operations'
import { UserSettingsAreaRouteContext } from '../UserSettingsArea'

import { createAccessToken } from './create'

import styles from './UserSettingsCreateAccessTokenCallbackPage.module.scss'

/**
 * Utility function to open the callback URL in Sourcegraph App. Used where
 * window.open or target="_blank" cannot be used.
 */
function tauriShellOpen(uri: string): void {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-member-access, @typescript-eslint/no-unsafe-call, @typescript-eslint/no-explicit-any
    ;(window as any).__TAURI__?.shell?.open(uri)
}

interface Props extends Pick<UserSettingsAreaRouteContext, 'authenticatedUser' | 'user'>, TelemetryProps {
    /**
     * Called when a new access token is created and should be temporarily displayed to the user.
     */
    onDidCreateAccessToken: (value: CreateAccessTokenResult['createAccessToken']) => void
    isSourcegraphDotCom: boolean
    isSourcegraphApp: boolean
}
interface TokenRequester {
    /** The name of the source */
    name: string
    /** The URL where the token should be added to */
    /** SECURITY: Local context only! Do not send token to non-local servers*/
    redirectURL: string
    /** The message to show users when the token has been created successfully */
    successMessage?: string
    /** The message to show users in case the token cannot be imported automatically */
    infoMessage?: string
    /** How the redirect URL should be open: open in same tab vs open in a new-tab */
    /** Default: Open link in same tab */
    callbackType?: 'open' | 'new-tab'
    /** If set, the requester is only allowed on dotcom */
    onlyDotCom?: boolean
    /** If true, it will forward the `destination` param to the redirect URL if it starts with / */
    forwardDestination?: boolean
}
// SECURITY: Only accept callback requests from requesters on this allowed list
const REQUESTERS: Record<string, TokenRequester> = {
    VSCEAUTH: {
        name: 'VS Code Extension',
        redirectURL: 'vscode://sourcegraph.sourcegraph?code=$TOKEN',
        successMessage: 'Now opening VS Code...',
        infoMessage:
            'Please make sure you have VS Code running on your machine if you do not see an open dialog in your browser.',
        callbackType: 'new-tab',
    },
    APP: {
        name: 'Sourcegraph App',
        redirectURL: 'sourcegraph://app/auth/callback?code=$TOKEN',
        successMessage: 'Now opening the Sourcegraph App...',
        infoMessage: 'You will be redirected to Sourcegraph App.',
        callbackType: 'open',
        onlyDotCom: true,
        forwardDestination: true,
    },
    CODY: {
        name: 'Sourcegraph Cody - VS Code Extension',
        redirectURL: 'vscode://sourcegraph.cody-ai?code=$TOKEN',
        successMessage: 'Now opening VS Code...',
        infoMessage:
            'Please make sure you have VS Code running on your machine if you do not see an open dialog in your browser.',
        callbackType: 'new-tab',
    },
}

export function isAccessTokenCallbackPage(): boolean {
    return location.pathname.startsWith('/users/') && location.pathname.endsWith('/settings/tokens/new/callback')
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
export const UserSettingsCreateAccessTokenCallbackPage: React.FC<Props> = ({
    telemetryService,
    onDidCreateAccessToken,
    user,
    isSourcegraphDotCom,
    isSourcegraphApp,
}) => {
    const isLightTheme = useIsLightTheme()
    const navigate = useNavigate()
    const location = useLocation()
    useEffect(() => {
        telemetryService.logPageView('NewAccessTokenCallback')
    }, [telemetryService])
    /** Get the requester from the url parameters if any */
    const requestFrom = new URLSearchParams(location.search).get('requestFrom')
    /** The validated requester where the callback request originally comes from. */
    const [requester, setRequester] = useState<TokenRequester | null | undefined>(undefined)
    /** The contents of the note input field. */
    const [note, setNote] = useState<string>('')
    /** The newly created token if any. */
    const [newToken, setNewToken] = useState('')

    // Check and Match URL Search Prams
    useEffect((): void => {
        // If a requester is already set, we don't need to run this effect
        if (requester) {
            return
        }

        // SECURITY: Verify if the request is coming from an allowlisted source
        const isRequestValid = requestFrom && requestFrom in REQUESTERS
        if (!isRequestValid || !requestFrom || requester !== undefined) {
            navigate('../..', { relative: 'path' })
            return
        }

        if (REQUESTERS[requestFrom].onlyDotCom && !isSourcegraphDotCom) {
            navigate('../..', { relative: 'path' })
            return
        }

        const nextRequester = { ...REQUESTERS[requestFrom] }

        if (nextRequester.forwardDestination) {
            const destination = new URLSearchParams(location.search).get('destination')
            // SECURITY: only destinations starting with a "/" are allowed to prevent an open redirect vulnerability.
            if (destination?.startsWith('/')) {
                const redirectURL = new URL(nextRequester.redirectURL)
                redirectURL.searchParams.set('destination', destination)
                nextRequester.redirectURL = redirectURL.toString()
            }
        }

        if (isSourcegraphApp) {
            // Append type=app to the url to indicate to the requester that the callback is fulfilled by App
            const redirectURL = new URL(nextRequester.redirectURL)
            redirectURL.searchParams.set('type', 'app')
            nextRequester.redirectURL = redirectURL.toString()
        }

        setRequester(nextRequester)
        setNote(REQUESTERS[requestFrom].name)
    }, [isSourcegraphDotCom, isSourcegraphApp, location.search, navigate, requestFrom, requester])

    /**
     * We use this to handle token creation request from redirections.
     * Don't create token if this page wasn't linked to from a valid
     * requester (e.g. VS Code extension).
     */
    const [onAuthorize, creationOrError] = useEventObservable(
        useCallback(
            (click: Observable<React.MouseEvent>) =>
                click.pipe(
                    switchMap(() =>
                        (requester ? createAccessToken(user.id, [AccessTokenScopes.UserAll], note) : NEVER).pipe(
                            tap(result => {
                                // SECURITY: If the request was from a valid requestor, redirect to the allowlisted redirect URL.
                                // SECURITY: Local context ONLY
                                if (requester) {
                                    onDidCreateAccessToken(result)
                                    setNewToken(result.token)
                                    const uri = replaceToken(requester?.redirectURL, result.token)

                                    // If we're in App, override the callbackType
                                    // because we need to use tauriShellOpen to open the
                                    // callback in a browser.
                                    // Then navigate back to the home page since App doesn't
                                    // have a back button or tab that can be closed.
                                    if (isSourcegraphApp) {
                                        tauriShellOpen(uri)
                                        navigate('/')
                                        return
                                    }

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
                        )
                    )
                ),
            [requester, user.id, note, onDidCreateAccessToken, isSourcegraphApp, navigate]
        )
    )

    if (!requester) {
        return null
    }

    return (
        <div className={styles.wrapper}>
            <BrandLogo className={styles.logo} isLightTheme={isLightTheme} variant="logo" />

            <Card className={styles.card}>
                <H2 as={H1} className={styles.heading}>
                    Authorize {requester.name}?
                </H2>

                <Text weight="bold">This grants access to:</Text>
                <ul>
                    <li>Your Sourcegraph.com account</li>
                    <li>Perform actions on your behalf</li>
                </ul>
                <Text>{requester.infoMessage}</Text>
                <Text>If you are not trying to connect {requester.name}, click cancel.</Text>

                {!newToken && (
                    <div className={styles.buttonRow}>
                        <LoaderButton
                            className="flex-1"
                            variant="primary"
                            label="Authorize"
                            loading={creationOrError === 'loading'}
                            onClick={onAuthorize}
                        />
                        <Button
                            className="flex-1"
                            variant="secondary"
                            to={location.pathname.replace(/\/new\/callback$/, '')}
                            disabled={creationOrError === 'loading'}
                            as={Link}
                        >
                            Cancel
                        </Button>
                    </div>
                )}

                {newToken && (
                    <>
                        <Text weight="bold">{requester.successMessage}</Text>
                        <details>
                            <summary>Authorization details</summary>
                            <div className="mt-2">
                                <Text>{requester.name} access token successfully generated.</Text>
                                <CopyableText className="test-access-token" text={newToken} />
                                <Text className="form-help text-muted" size="small">
                                    This is a one-time access token to connect your account to {requester.name}. You
                                    will not be able to see this token again once the window is closed.
                                </Text>
                            </div>
                        </details>
                    </>
                )}

                {isErrorLike(creationOrError) && <ErrorAlert className="my-3 " error={creationOrError} />}
            </Card>
        </div>
    )
}

function replaceToken(url: string, token: string): string {
    // %24 is the URL encoded version of $
    return url.replace('$TOKEN', token).replace('%24TOKEN', token)
}
