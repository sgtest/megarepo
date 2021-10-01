import classNames from 'classnames'
import ArrowRightIcon from 'mdi-react/ArrowRightIcon'
import React from 'react'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { MarketingBlock } from '@sourcegraph/web/src/components/MarketingBlock'

export interface SelfHostedCtaProps extends TelemetryProps {
    className?: string
    contentClassName?: string
    page: string
}

export const SelfHostedCta: React.FunctionComponent<SelfHostedCtaProps> = ({
    className,
    contentClassName,
    telemetryService,
    page,
    children,
}) => {
    const linkProps = { rel: 'noopener noreferrer' }

    const gettingStartedCTAOnClick = (): void => {
        telemetryService.log('InstallSourcegraphCTAClicked', { page })
    }

    const selfVsCloudDocumentsLinkOnClick = (): void => {
        telemetryService.log('SelfVsCloudDocsLink', { page })
    }

    const helpGettingStartedCTAOnClick = (): void => {
        telemetryService.log('HelpGettingStartedCTA', { page })
    }

    return (
        <div
            className={classNames(
                'd-flex flex-md-row align-items-md-start justify-content-md-between flex-column',
                className
            )}
        >
            <div className={classNames('mr-md-4 mr-0', contentClassName)}>
                {children}

                <ul>
                    <li>
                        <a
                            onClick={gettingStartedCTAOnClick}
                            href="https://docs.sourcegraph.com/admin/install"
                            {...linkProps}
                        >
                            Learn how to install
                        </a>
                    </li>
                    <li>
                        <a
                            onClick={selfVsCloudDocumentsLinkOnClick}
                            href="https://docs.sourcegraph.com/code_search/explanations/sourcegraph_cloud#who-is-sourcegraph-cloud-for-why-should-i-use-this-over-sourcegraph-self-hosted"
                            {...linkProps}
                        >
                            Self-hosted vs. cloud features
                        </a>
                    </li>
                </ul>
            </div>

            <MarketingBlock wrapperClassName="flex-md-shrink-0 mt-md-0 mt-sm-2 w-sm-100">
                <h3 className="pr-3">Need help getting started?</h3>

                <div>
                    <a
                        onClick={helpGettingStartedCTAOnClick}
                        href="https://info.sourcegraph.com/talk-to-a-developer?_ga=2.257481099.1451692402.1630329056-789994471.1629391417"
                        {...linkProps}
                    >
                        Speak to an engineer
                        <ArrowRightIcon className="icon-inline ml-2" />
                    </a>
                </div>
            </MarketingBlock>
        </div>
    )
}
