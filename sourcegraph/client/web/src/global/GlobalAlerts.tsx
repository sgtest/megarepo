import React from 'react'

import classNames from 'classnames'
import { parseISO } from 'date-fns'
import differenceInDays from 'date-fns/differenceInDays'

import { renderMarkdown } from '@sourcegraph/common'
import { gql, useQuery } from '@sourcegraph/http-client'
import { useSettings } from '@sourcegraph/shared/src/settings/settings'
import { Link, Markdown } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { DismissibleAlert } from '../components/DismissibleAlert'
import { GlobalAlertsSiteFlagsResult, GlobalAlertsSiteFlagsVariables } from '../graphql-operations'
import { FreeUsersExceededAlert } from '../site/FreeUsersExceededAlert'
import { LicenseExpirationAlert } from '../site/LicenseExpirationAlert'
import { NeedsRepositoryConfigurationAlert } from '../site/NeedsRepositoryConfigurationAlert'
import { siteFlagFieldsFragment } from '../storm/pages/LayoutPage/LayoutPage.loader'

import { GlobalAlert } from './GlobalAlert'
import { Notices, VerifyEmailNotices } from './Notices'

import styles from './GlobalAlerts.module.scss'

interface Props {
    authenticatedUser: AuthenticatedUser | null
    isSourcegraphApp: boolean
}

// NOTE: The name of the query is also added in the refreshSiteFlags() function
// found in client/web/src/site/backend.tsx
const QUERY = gql`
    query GlobalAlertsSiteFlags {
        site {
            ...SiteFlagFields
        }
    }

    ${siteFlagFieldsFragment}
`
/**
 * Fetches and displays relevant global alerts at the top of the page
 */
export const GlobalAlerts: React.FunctionComponent<Props> = ({ authenticatedUser, isSourcegraphApp }) => {
    const settings = useSettings()
    const { data } = useQuery<GlobalAlertsSiteFlagsResult, GlobalAlertsSiteFlagsVariables>(QUERY, {
        fetchPolicy: 'cache-and-network',
    })
    const siteFlagsValue = data?.site

    return (
        <div className={classNames('test-global-alert', styles.globalAlerts)}>
            {siteFlagsValue && (
                <>
                    {siteFlagsValue?.externalServicesCounts.remoteExternalServicesCount === 0 && !isSourcegraphApp && (
                        <NeedsRepositoryConfigurationAlert className={styles.alert} />
                    )}
                    {siteFlagsValue.freeUsersExceeded && (
                        <FreeUsersExceededAlert
                            noLicenseWarningUserCount={siteFlagsValue.productSubscription.noLicenseWarningUserCount}
                            className={styles.alert}
                        />
                    )}
                    {siteFlagsValue.alerts.map((alert, index) => (
                        <GlobalAlert key={index} alert={alert} className={styles.alert} />
                    ))}
                    {siteFlagsValue.productSubscription.license &&
                        (() => {
                            const expiresAt = parseISO(siteFlagsValue.productSubscription.license.expiresAt)
                            return (
                                differenceInDays(expiresAt, Date.now()) <= 7 && (
                                    <LicenseExpirationAlert
                                        expiresAt={expiresAt}
                                        daysLeft={Math.floor(differenceInDays(expiresAt, Date.now()))}
                                        className={styles.alert}
                                    />
                                )
                            )
                        })()}
                </>
            )}
            {settings?.motd &&
                Array.isArray(settings.motd) &&
                settings.motd.map(motd => (
                    <DismissibleAlert
                        key={motd}
                        partialStorageKey={`motd.${motd}`}
                        variant="info"
                        className={styles.alert}
                    >
                        <Markdown dangerousInnerHTML={renderMarkdown(motd)} />
                    </DismissibleAlert>
                ))}
            {process.env.SOURCEGRAPH_API_URL && (
                <DismissibleAlert
                    key="dev-web-server-alert"
                    partialStorageKey="dev-web-server-alert"
                    variant="danger"
                    className={styles.alert}
                >
                    <div>
                        <strong>Warning!</strong> This build uses data from the proxied API:{' '}
                        <Link className={styles.proxyLink} target="__blank" to={process.env.SOURCEGRAPH_API_URL}>
                            {process.env.SOURCEGRAPH_API_URL}
                        </Link>
                    </div>
                    .
                </DismissibleAlert>
            )}
            <Notices alertClassName={styles.alert} location="top" />
            <VerifyEmailNotices authenticatedUser={authenticatedUser} alertClassName={styles.alert} />
        </div>
    )
}
