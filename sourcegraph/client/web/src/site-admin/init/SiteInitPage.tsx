import React from 'react'

import { Navigate } from 'react-router-dom'

import { logger } from '@sourcegraph/common'
import { useIsLightTheme } from '@sourcegraph/shared/src/theme'
import { CardBody, Card, H2, Text } from '@sourcegraph/wildcard'

import type { AuthenticatedUser } from '../../auth'
import { type SignUpArguments, SignUpForm } from '../../auth/SignUpForm'
import { BrandLogo } from '../../components/branding/BrandLogo'
import type { SourcegraphContext } from '../../jscontext'
import { PageRoutes } from '../../routes.constants'

import styles from './SiteInitPage.module.scss'

const initSite = async (args: SignUpArguments): Promise<void> => {
    const pingUrl = new URL('https://sourcegraph.com/ping-from-self-hosted')
    pingUrl.searchParams.set('email', args.email)
    pingUrl.searchParams.set('tos_accepted', 'true') // Terms of Service are required to be accepted

    await fetch(pingUrl.toString(), {
        credentials: 'include',
    })
        .then() // no op
        .catch((error): void => {
            logger.error(error)
        })
    const response = await fetch('/-/site-init', {
        credentials: 'same-origin',
        method: 'POST',
        headers: {
            ...window.context.xhrHeaders,
            Accept: 'application/json',
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(args),
    })
    if (response.status !== 200) {
        const text = await response.text()
        throw new Error(text)
    }
    window.location.replace('/site-admin')
}

interface Props {
    authenticatedUser: Pick<AuthenticatedUser, 'username'> | null

    /**
     * Whether site initialization is needed. If not set, the global value from
     * `window.context.needsSiteInit` is used.
     */
    needsSiteInit?: typeof window.context.needsSiteInit
    context: Pick<
        SourcegraphContext,
        'sourcegraphDotComMode' | 'authProviders' | 'experimentalFeatures' | 'authMinPasswordLength'
    >
}

/**
 * A page that is shown when the Sourcegraph instance has not yet been initialized.
 * Only the person who first accesses the instance will see this.
 */
export const SiteInitPage: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    authenticatedUser,
    needsSiteInit = window.context.needsSiteInit,
    context,
}) => {
    const isLightTheme = useIsLightTheme()

    if (!needsSiteInit) {
        return <Navigate to={PageRoutes.Search} replace={true} />
    }

    return (
        <div className={styles.siteInitPage}>
            <Card className={styles.content}>
                <CardBody className="p-4">
                    <BrandLogo className="w-100 mb-3" isLightTheme={isLightTheme} variant="logo" />
                    {authenticatedUser ? (
                        // If there's already a user but the site is not initialized, then the we're in an
                        // unexpected state, likely because of a previous bug or because someone manually modified
                        // the site_config DB table.
                        <Text>
                            You're signed in as <strong>{authenticatedUser.username}</strong>. A site admin must
                            initialize Sourcegraph before you can continue.
                        </Text>
                    ) : (
                        <>
                            <H2 className="site-init-page__header">Welcome</H2>
                            <Text>Create an admin account to start using Sourcegraph.</Text>
                            <SignUpForm
                                className="w-100"
                                buttonLabel="Create admin account & continue"
                                onSignUp={initSite}
                                context={context}
                            />
                        </>
                    )}
                </CardBody>
            </Card>
        </div>
    )
}
