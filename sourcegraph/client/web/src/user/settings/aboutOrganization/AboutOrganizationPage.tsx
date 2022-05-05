import React, { useEffect } from 'react'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { PageHeader } from '@sourcegraph/wildcard'

import { PageTitle } from '../../../components/PageTitle'
import { SelfHostedCta } from '../../../components/SelfHostedCta'

import styles from './AboutOrganizationPage.module.scss'
interface AboutOrganizationPageProps extends TelemetryProps {}

export const AboutOrganizationPage: React.FunctionComponent<React.PropsWithChildren<AboutOrganizationPageProps>> = ({
    telemetryService,
}) => {
    useEffect(() => {
        telemetryService.logViewEvent('AboutOrg')
    }, [telemetryService])

    return (
        <>
            <PageTitle title="Organizations" />
            <PageHeader
                headingElement="h2"
                path={[{ text: 'Organizations' }]}
                description="Support for organizations is not currently available on Sourcegraph Cloud."
                className="mb-3"
            />
            <SelfHostedCta
                contentClassName={styles.selfHostedCtaContent}
                page="organizations"
                telemetryService={telemetryService}
            >
                <p className="mb-2">
                    <strong>Need more enterprise features? Run Sourcegraph self-hosted</strong>
                </p>
                <p className="mb-2">
                    For additional code hosts and enterprise only features, install Sourcegraph self-hosted.
                </p>
            </SelfHostedCta>
        </>
    )
}
