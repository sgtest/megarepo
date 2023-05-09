import React, { useState, useCallback } from 'react'

import { mdiDotsHorizontal } from '@mdi/js'
import classNames from 'classnames'

import { Timestamp } from '@sourcegraph/branded/src/components/Timestamp'
import { Button, Link, Icon, Code } from '@sourcegraph/wildcard'

import { eventLogger } from '../../tracking/eventLogger'
import { CommitMessageWithLinks } from '../commit/CommitMessageWithLinks'
import { Linkified } from '../linkifiy/Linkified'

import { GitCommitNodeProps } from './GitCommitNode'
import { GitCommitNodeByline } from './GitCommitNodeByline'

import styles from './GitCommitNode.module.scss'

export const GitCommitNodeTableRow: React.FC<
    Omit<
        GitCommitNodeProps,
        | 'wrapperElement'
        | 'afterElement'
        | 'preferAbsoluteTimestamps'
        | 'showSHAAndParentsRow'
        | 'onHandleDiffMode'
        | 'diffMode'
    >
> = ({ node, className, expandCommitMessageBody, hideExpandCommitMessageBody, messageSubjectClassName }) => {
    const [showCommitMessageBody, setShowCommitMessageBody] = useState<boolean>(false)

    const toggleShowCommitMessageBody = useCallback((): void => {
        eventLogger.log('CommitBodyToggled')
        setShowCommitMessageBody(!showCommitMessageBody)
    }, [showCommitMessageBody])

    const messageElement = (
        <div className={classNames(styles.message, styles.messageSmall)} data-testid="git-commit-node-message">
            <span className={classNames('mr-2', styles.messageSubject)}>
                <CommitMessageWithLinks
                    to={node.canonicalURL}
                    className={classNames(messageSubjectClassName, styles.messageLink)}
                    message={node.subject}
                    externalURLs={node.externalURLs}
                />
            </span>
            {node.body && !hideExpandCommitMessageBody && !expandCommitMessageBody && (
                <Button
                    className={styles.messageToggle}
                    onClick={toggleShowCommitMessageBody}
                    variant="secondary"
                    size="sm"
                    aria-label={showCommitMessageBody ? 'Hide commit message body' : 'Show commit message body'}
                >
                    <Icon aria-hidden={true} svgPath={mdiDotsHorizontal} />
                </Button>
            )}

            <small className={classNames('text-muted', styles.messageTimestamp)}>
                <Timestamp noAbout={true} date={node.committer ? node.committer.date : node.author.date} />
            </small>
        </div>
    )

    const commitMessageBody =
        expandCommitMessageBody || showCommitMessageBody ? (
            <tr className={classNames(styles.tableRow, className)}>
                <td colSpan={3}>
                    <pre className={styles.messageBody}>
                        {node.body && <Linkified input={node.body} externalURLs={node.externalURLs} />}
                    </pre>
                </td>
            </tr>
        ) : undefined

    return (
        <>
            <tr
                className={classNames(styles.tableRow, 'px-1', className, {
                    [styles.tableRowOpen]: commitMessageBody !== undefined,
                })}
            >
                <GitCommitNodeByline
                    as="td"
                    className={classNames('d-flex', styles.colByline)}
                    avatarClassName={styles.fontWeightNormal}
                    author={node.author}
                    committer={node.committer}
                    compact={true}
                />
                <td className="flex-1 overflow-hidden">{messageElement}</td>
                <td className="text-right">
                    <Link to={node.canonicalURL}>
                        <Code data-testid="git-commit-node-oid">
                            {node.perforceChangelist?.cid ?? node.abbreviatedOID}
                        </Code>
                    </Link>
                </td>
            </tr>
            {commitMessageBody}
        </>
    )
}
