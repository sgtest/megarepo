import classNames from 'classnames'
import copy from 'copy-to-clipboard'
import ContentCopyIcon from 'mdi-react/ContentCopyIcon'
import DownloadIcon from 'mdi-react/DownloadIcon'
import OpenInNewIcon from 'mdi-react/OpenInNewIcon'
import React, { useState } from 'react'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'

import styles from './SelfHostInstructions.module.scss'

export const SelfHostInstructions: React.FunctionComponent<TelemetryProps> = ({ telemetryService }) => {
    const dockerCommand =
        'docker run --publish 7080:7080 --publish 127.0.0.1:3370:3370 --rm --volume ~/.sourcegraph/config:/etc/sourcegraph --volume ~/.sourcegraph/data:/var/opt/sourcegraph sourcegraph/server:3.30.3'

    const copyTooltip = 'Copy command'
    const copyCompletedTooltip = 'Copied!'

    const [currentCopyTooltip, setCurrentCopyTooltip] = useState(copyTooltip)

    const onCopy = (): void => {
        telemetryService.log('HomepageCTAClicked', { campaign: 'Local install' }, { campaign: 'Local install' })
        copy(dockerCommand)
        setCurrentCopyTooltip(copyCompletedTooltip)
        setTimeout(() => setCurrentCopyTooltip(copyTooltip), 1000)
    }

    const onTalkToEngineerClicked = (): void => {
        telemetryService.log(
            'HomepageCTAClicked',
            { campaign: 'Talk to an engineer' },
            { campaign: 'Talk to an engineer' }
        )
    }

    return (
        <div className={styles.wrapper}>
            <div className={styles.column}>
                <h2>
                    <DownloadIcon className={classNames('icon-inline mr-2', styles.downloadIcon)} /> Self-hosted
                    deployment
                </h2>
                <ul className={styles.featureList}>
                    <li>Free for up to 10 users</li>
                    <li>Supports additional (and local) code hosts</li>
                    <li>Team oriented functionality</li>
                    <li>Your code never leaves your server</li>
                    <li>Free 30 day trial of enterprise-only features</li>
                </ul>
                <a
                    href="https://docs.sourcegraph.com/cloud/cloud_ent_on-prem_comparison"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    Learn more about self-hosted vs. cloud features{' '}
                    <OpenInNewIcon aria-label="Open in new window" className="icon-inline" />
                </a>
            </div>

            <div className={styles.column}>
                <div>
                    <strong>Quickstart:</strong> launch Sourcegraph at http://localhost:3370
                </div>
                <div className={styles.codeWrapper}>
                    <button
                        type="button"
                        className={classNames('btn btn-link', styles.copyButton)}
                        onClick={onCopy}
                        data-tooltip={currentCopyTooltip}
                        data-placement="top"
                        aria-label="Copy Docker command to clipboard"
                    >
                        <ContentCopyIcon className="icon-inline" />
                    </button>
                    <code className={styles.code}>{dockerCommand}</code>
                </div>
                <div className="d-flex justify-content-between">
                    <a
                        href="https://docs.sourcegraph.com/admin/install"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="mr-2"
                    >
                        Learn how to deploy a server or cluster{' '}
                        <OpenInNewIcon aria-label="Open in new window" className="icon-inline" />
                    </a>
                    <a
                        href="https://about.sourcegraph.com/contact/request-info/"
                        onClick={onTalkToEngineerClicked}
                        className="text-right flex-shrink-0"
                    >
                        Talk to an engineer
                    </a>
                </div>
            </div>
        </div>
    )
}
