import React, { useEffect } from 'react'

import classNames from 'classnames'

import { AuthenticatedUser } from '@sourcegraph/shared/src/auth'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Button, Card, CardBody, Link, PageHeader } from '@sourcegraph/wildcard'

import { CallToActionBanner } from '../../../../../components/CallToActionBanner'
import { Page } from '../../../../../components/Page'
import { PageTitle } from '../../../../../components/PageTitle'
import { CodeInsightsIcon } from '../../../../../insights/Icons'
import { eventLogger } from '../../../../../tracking/eventLogger'
import { CodeInsightsLandingPageContext, CodeInsightsLandingPageType } from '../CodeInsightsLandingPageContext'
import { CodeInsightsDescription } from '../getting-started/components/code-insights-description/CodeInsightsDescription'

import { CodeInsightsExamplesPicker } from './components/code-insights-examples-picker/CodeInsightsExamplesPicker'

import styles from './CodeInsightsDotComGetStarted.module.scss'

const DOT_COM_CONTEXT = { mode: CodeInsightsLandingPageType.Cloud }

export interface CodeInsightsDotComGetStartedProps extends TelemetryProps {
    authenticatedUser: AuthenticatedUser | null
}

export const CodeInsightsDotComGetStarted: React.FunctionComponent<
    React.PropsWithChildren<CodeInsightsDotComGetStartedProps>
> = props => {
    const { telemetryService } = props
    const isSourcegraphDotCom = window.context.sourcegraphDotComMode

    useEffect(() => {
        telemetryService.logViewEvent('CloudInsightsGetStartedPage')
    }, [telemetryService])

    return (
        <CodeInsightsLandingPageContext.Provider value={DOT_COM_CONTEXT}>
            <Page>
                <PageTitle title="Code Insights" />
                <PageHeader
                    path={[{ icon: CodeInsightsIcon, text: 'Insights' }]}
                    actions={
                        isSourcegraphDotCom ? (
                            <Button
                                as={Link}
                                to="https://about.sourcegraph.com"
                                variant="primary"
                                onClick={() => eventLogger.log('ClickedOnEnterpriseCTA', { location: 'TryInsights' })}
                            >
                                Get Sourcegraph Enterprise
                            </Button>
                        ) : null
                    }
                    className="mb-4"
                />
                <main className="pb-5">
                    <Card as={CardBody} className={styles.heroSection}>
                        <aside className={styles.heroVideoBlock}>
                            <video
                                className={classNames('shadow percy-hide w-100 h-auto')}
                                width={1280}
                                height={720}
                                autoPlay={true}
                                muted={true}
                                loop={true}
                                playsInline={true}
                                controls={false}
                            >
                                <source
                                    type="video/webm"
                                    src="https://storage.googleapis.com/sourcegraph-assets/code_insights/code-insights-720.webm"
                                />

                                <source
                                    type="video/mp4"
                                    src="https://storage.googleapis.com/sourcegraph-assets/code_insights/code-insights-720.mp4"
                                />
                            </video>
                        </aside>
                        <CodeInsightsDescription className={styles.heroDescriptionBlock} />
                    </Card>

                    <CallToActionBanner variant="filled">
                        To track Insights across your team's private repositories,{' '}
                        <Link
                            to="https://about.sourcegraph.com"
                            onClick={() => eventLogger.log('ClickedOnEnterpriseCTA', { location: 'Insights' })}
                        >
                            get Sourcegraph Enterprise
                        </Link>
                        .
                    </CallToActionBanner>

                    <CodeInsightsExamplesPicker telemetryService={telemetryService} />
                </main>
            </Page>
        </CodeInsightsLandingPageContext.Provider>
    )
}
