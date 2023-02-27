import React, { useEffect, useState } from 'react'

import { mdiBitbucket, mdiGithub, mdiGitlab, mdiEmail, mdiMicrosoftAzureDevops } from '@mdi/js'
import classNames from 'classnames'
import { partition } from 'lodash'
import { Navigate, useLocation, useSearchParams } from 'react-router-dom'

import { Alert, Icon, Text, Link, Button, ErrorAlert, AnchorLink } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { HeroPage } from '../components/HeroPage'
import { PageTitle } from '../components/PageTitle'
import { AuthProvider, SourcegraphContext } from '../jscontext'
import { eventLogger } from '../tracking/eventLogger'
import { checkRequestAccessAllowed } from '../util/checkRequestAccessAllowed'

import { SourcegraphIcon } from './icons'
import { OrDivider } from './OrDivider'
import { getReturnTo } from './SignInSignUpCommon'
import { UsernamePasswordSignInForm } from './UsernamePasswordSignInForm'

import signInSignUpCommonStyles from './SignInSignUpCommon.module.scss'

interface SignInPageProps {
    authenticatedUser: AuthenticatedUser | null
    context: Pick<
        SourcegraphContext,
        | 'allowSignup'
        | 'authProviders'
        | 'sourcegraphDotComMode'
        | 'xhrHeaders'
        | 'resetPasswordEnabled'
        | 'experimentalFeatures'
    >
    isSourcegraphDotCom: boolean
}

export const SignInPage: React.FunctionComponent<React.PropsWithChildren<SignInPageProps>> = props => {
    const { isSourcegraphDotCom, context, authenticatedUser } = props
    useEffect(() => eventLogger.logViewEvent('SignIn', null, false))

    const location = useLocation()
    const [error, setError] = useState<Error | null>(null)
    const [searchParams] = useSearchParams()
    const isRequestAccessAllowed = checkRequestAccessAllowed(
        isSourcegraphDotCom,
        context.allowSignup,
        context.experimentalFeatures
    )

    if (authenticatedUser) {
        const returnTo = getReturnTo(location)
        return <Navigate to={returnTo} replace={true} />
    }

    const [[builtInAuthProvider], nonBuiltinAuthProviders] = partition(
        context.authProviders,
        provider => provider.isBuiltin
    )

    const shouldShowProvider = function (provider: AuthProvider): boolean {
        // Hide the Sourcegraph Operator authentication provider by default because it is
        // not useful to customer users and may even cause confusion.
        if (provider.serviceType === 'sourcegraph-operator') {
            return searchParams.has('sourcegraph-operator')
        }
        if (provider.serviceType === 'gerrit') {
            return false
        }
        return true
    }

    const thirdPartyAuthProviders = nonBuiltinAuthProviders.filter(provider => shouldShowProvider(provider))

    const showBuiltInAuthForm = searchParams.has('email') || thirdPartyAuthProviders.length === 0

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
                    {builtInAuthProvider && showBuiltInAuthForm && (
                        <UsernamePasswordSignInForm
                            {...props}
                            onAuthError={setError}
                            className={classNames({ 'mb-3': thirdPartyAuthProviders.length > 0 })}
                        />
                    )}
                    {builtInAuthProvider && showBuiltInAuthForm && thirdPartyAuthProviders.length > 0 && (
                        <OrDivider className="mb-3 py-1" />
                    )}
                    {thirdPartyAuthProviders.map((provider, index) => (
                        // Use index as key because display name may not be unique. This is OK
                        // here because this list will not be updated during this component's lifetime.
                        /* eslint-disable react/no-array-index-key */
                        <div className="mb-2" key={index}>
                            <Button
                                to={provider.authenticationURL}
                                display="block"
                                variant={showBuiltInAuthForm ? 'secondary' : 'primary'}
                                as={AnchorLink}
                            >
                                {provider.serviceType === 'github' && <Icon aria-hidden={true} svgPath={mdiGithub} />}
                                {provider.serviceType === 'gitlab' && <Icon aria-hidden={true} svgPath={mdiGitlab} />}
                                {provider.serviceType === 'bitbucketCloud' && (
                                    <Icon aria-hidden={true} svgPath={mdiBitbucket} />
                                )}
                                {provider.serviceType === 'azuredevops' && (
                                    <Icon aria-hidden={true} svgPath={mdiMicrosoftAzureDevops} />
                                )}{' '}
                                Continue with {provider.displayName}
                            </Button>
                        </div>
                    ))}
                    {builtInAuthProvider && !showBuiltInAuthForm && (
                        <div className="mb-2">
                            <Button
                                to={`${location.pathname}?email=1&${searchParams.toString()}`}
                                display="block"
                                variant="secondary"
                                as={Link}
                            >
                                <Icon aria-hidden={true} svgPath={mdiEmail} /> Continue with Email
                            </Button>
                        </div>
                    )}
                </div>
                {context.allowSignup ? (
                    <Text>
                        New to Sourcegraph? <Link to="/sign-up">Sign up.</Link>{' '}
                        {isSourcegraphDotCom && (
                            <>
                                To use Sourcegraph on private repositories,
                                <Link
                                    to="https://signup.sourcegraph.com"
                                    onClick={() =>
                                        eventLogger.log('ClickedOnEnterpriseCTA', { location: 'SignInPage' })
                                    }
                                >
                                    get Sourcegraph Enterprise
                                </Link>
                                .
                            </>
                        )}
                    </Text>
                ) : isRequestAccessAllowed ? (
                    <Text className="text-muted">
                        Need an account? <Link to="/request-access">Request access</Link> or contact your site admin.
                    </Text>
                ) : (
                    <Text className="text-muted">Need an account? Contact your site admin.</Text>
                )}
            </div>
        )

    return (
        <div className={signInSignUpCommonStyles.signinSignupPage}>
            <PageTitle title="Sign in" />
            <HeroPage
                icon={SourcegraphIcon}
                iconLinkTo={context.sourcegraphDotComMode ? '/search' : undefined}
                iconClassName="bg-transparent"
                lessPadding={true}
                title="Sign in to Sourcegraph"
                body={body}
            />
        </div>
    )
}
