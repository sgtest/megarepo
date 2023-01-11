import React, { useEffect, useState } from 'react'

import { mdiGithub, mdiGitlab } from '@mdi/js'
import classNames from 'classnames'
import { partition } from 'lodash'
import { Navigate, useLocation } from 'react-router-dom-v5-compat'

import { Alert, Icon, Text, Link, AnchorLink, Button, ErrorAlert } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { HeroPage } from '../components/HeroPage'
import { PageTitle } from '../components/PageTitle'
import { AuthProvider, SourcegraphContext } from '../jscontext'
import { eventLogger } from '../tracking/eventLogger'

import { SourcegraphIcon } from './icons'
import { OrDivider } from './OrDivider'
import { getReturnTo } from './SignInSignUpCommon'
import { UsernamePasswordSignInForm } from './UsernamePasswordSignInForm'

import signInSignUpCommonStyles from './SignInSignUpCommon.module.scss'

interface SignInPageProps {
    authenticatedUser: AuthenticatedUser | null
    context: Pick<
        SourcegraphContext,
        'allowSignup' | 'authProviders' | 'sourcegraphDotComMode' | 'xhrHeaders' | 'resetPasswordEnabled'
    >
    isSourcegraphDotCom: boolean
}

export const SignInPage: React.FunctionComponent<React.PropsWithChildren<SignInPageProps>> = props => {
    useEffect(() => eventLogger.logViewEvent('SignIn', null, false))

    const location = useLocation()
    const [error, setError] = useState<Error | null>(null)

    if (props.authenticatedUser) {
        const returnTo = getReturnTo(location)
        return <Navigate to={returnTo} replace={true} />
    }

    const [[builtInAuthProvider], nonBuiltinAuthProviders] = partition(
        props.context.authProviders,
        provider => provider.isBuiltin
    )

    const shouldShowProvider = function (provider: AuthProvider): boolean {
        // Hide the Sourcegraph Operator authentication provider by default because it is
        // not useful to customer users and may even cause confusion.
        if (provider.serviceType === 'sourcegraph-operator') {
            return new URLSearchParams(location.search).has('sourcegraph-operator')
        }
        return true
    }

    const thirdPartyAuthProviders = nonBuiltinAuthProviders.filter(provider => shouldShowProvider(provider))

    const body =
        !builtInAuthProvider && thirdPartyAuthProviders.length === 0 ? (
            <Alert className="mt-3" variant="info">
                No authentication providers are available. Contact a site administrator for help.
            </Alert>
        ) : (
            <div className={classNames('mb-4 pb-5', signInSignUpCommonStyles.signinPageContainer)}>
                {error && <ErrorAlert className="mt-4 mb-0 text-left" error={error} />}
                <div
                    className={classNames(
                        'test-signin-form rounded p-4 my-3',
                        signInSignUpCommonStyles.signinSignupForm,
                        error ? 'mt-3' : 'mt-4'
                    )}
                >
                    {builtInAuthProvider && (
                        <UsernamePasswordSignInForm
                            {...props}
                            onAuthError={setError}
                            noThirdPartyProviders={thirdPartyAuthProviders.length === 0}
                        />
                    )}
                    {builtInAuthProvider && thirdPartyAuthProviders.length > 0 && <OrDivider className="mb-3 py-1" />}
                    {thirdPartyAuthProviders.map((provider, index) => (
                        // Use index as key because display name may not be unique. This is OK
                        // here because this list will not be updated during this component's lifetime.
                        /* eslint-disable react/no-array-index-key */
                        <div className="mb-2" key={index}>
                            <Button to={provider.authenticationURL} display="block" variant="secondary" as={AnchorLink}>
                                {provider.serviceType === 'github' && (
                                    <>
                                        <Icon aria-hidden={true} svgPath={mdiGithub} />{' '}
                                    </>
                                )}
                                {provider.serviceType === 'gitlab' && (
                                    <>
                                        <Icon aria-hidden={true} svgPath={mdiGitlab} />{' '}
                                    </>
                                )}
                                Continue with {provider.displayName}
                            </Button>
                        </div>
                    ))}
                </div>
                {props.context.allowSignup ? (
                    <Text>
                        New to Sourcegraph?{' '}
                        {props.isSourcegraphDotCom ? (
                            <Link
                                to="https://signup.sourcegraph.com"
                                target="_blank"
                                rel="noopener noreferrer"
                                onClick={() =>
                                    eventLogger.log('ClickedOnCloudCTA', { cloudCtaType: 'NavBarLoggedOut' })
                                }
                            >
                                Sign up
                            </Link>
                        ) : (
                            <Link to="/sign-up">Sign up</Link>
                        )}
                    </Text>
                ) : (
                    <Text className="text-muted">Need an account? Contact your site admin</Text>
                )}
            </div>
        )

    return (
        <div className={signInSignUpCommonStyles.signinSignupPage}>
            <PageTitle title="Sign in" />
            <HeroPage
                icon={SourcegraphIcon}
                iconLinkTo={props.context.sourcegraphDotComMode ? '/search' : undefined}
                iconClassName="bg-transparent"
                lessPadding={true}
                title="Sign in to Sourcegraph"
                body={body}
            />
        </div>
    )
}
