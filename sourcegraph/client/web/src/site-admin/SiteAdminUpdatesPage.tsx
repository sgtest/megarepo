import React, { useMemo } from 'react'

import { mdiCloudDownload } from '@mdi/js'
import { parseISO } from 'date-fns'
import formatDistance from 'date-fns/formatDistance'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { isErrorLike } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { LoadingSpinner, useObservable, Link, Alert, Icon, Code, H2, Text } from '@sourcegraph/wildcard'

import { PageTitle } from '../components/PageTitle'

import { fetchSiteUpdateCheck } from './backend'

import styles from './SiteAdminUpdatesPage.module.scss'

interface Props extends TelemetryProps {}

/**
 * A page displaying information about available updates for the server.
 */
export const SiteAdminUpdatesPage: React.FunctionComponent<React.PropsWithChildren<Props>> = ({ telemetryService }) => {
    useMemo(() => {
        telemetryService.logViewEvent('SiteAdminUpdates')
    }, [telemetryService])

    const state = useObservable(useMemo(() => fetchSiteUpdateCheck(), []))
    const autoUpdateCheckingEnabled = window.context.site['update.channel'] === 'release'

    if (state === undefined) {
        return <LoadingSpinner />
    }

    const updateCheck = state.updateCheck

    return (
        <div>
            <PageTitle title="Updates - Admin" />
            <H2>Updates</H2>
            {isErrorLike(state) && <ErrorAlert error={state} />}
            {updateCheck && (updateCheck.pending || updateCheck.checkedAt) && (
                <div>
                    {updateCheck.pending && (
                        <Alert className={styles.alert} variant="primary">
                            <LoadingSpinner /> Checking for updates... (reload in a few seconds)
                        </Alert>
                    )}
                    {!updateCheck.errorMessage &&
                        (updateCheck.updateVersionAvailable ? (
                            <Alert className={styles.alert} variant="success">
                                <Icon aria-hidden={true} svgPath={mdiCloudDownload} /> Update available:{' '}
                                <Link to="https://about.sourcegraph.com">{updateCheck.updateVersionAvailable}</Link>
                            </Alert>
                        ) : (
                            <Alert className={styles.alert} variant="success">
                                Up to date.
                            </Alert>
                        ))}
                    {updateCheck.errorMessage && (
                        <ErrorAlert
                            className={styles.alert}
                            prefix="Error checking for updates"
                            error={updateCheck.errorMessage}
                        />
                    )}
                </div>
            )}

            {!autoUpdateCheckingEnabled && (
                <Alert className={styles.alert} variant="warning">
                    Automatic update checking is disabled.
                </Alert>
            )}

            <Text className="site-admin-updates_page__info">
                <small>
                    <strong>Current product version:</strong> {state.productVersion} ({state.buildVersion})
                </small>
                <br />
                <small>
                    <strong>Last update check:</strong>{' '}
                    {updateCheck.checkedAt
                        ? formatDistance(parseISO(updateCheck.checkedAt), new Date(), {
                              addSuffix: true,
                          })
                        : 'never'}
                    .
                </small>
                <br />
                <small>
                    <strong>Automatic update checking:</strong> {autoUpdateCheckingEnabled ? 'on' : 'off'}.{' '}
                    <Link to="/site-admin/configuration">Configure</Link> <Code>update.channel</Code> to{' '}
                    {autoUpdateCheckingEnabled ? 'disable' : 'enable'}.
                </small>
            </Text>
            <Text>
                <Link to="https://about.sourcegraph.com/changelog" target="_blank" rel="noopener">
                    Sourcegraph changelog
                </Link>
            </Text>
        </div>
    )
}
