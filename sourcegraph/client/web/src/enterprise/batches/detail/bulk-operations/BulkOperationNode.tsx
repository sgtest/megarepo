import React from 'react'

import classNames from 'classnames'
import CommentOutlineIcon from 'mdi-react/CommentOutlineIcon'
import ExternalLinkIcon from 'mdi-react/ExternalLinkIcon'
import LinkVariantRemoveIcon from 'mdi-react/LinkVariantRemoveIcon'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'
import SyncIcon from 'mdi-react/SyncIcon'
import UploadIcon from 'mdi-react/UploadIcon'

import { ErrorMessage } from '@sourcegraph/branded/src/components/alerts'
import { pluralize } from '@sourcegraph/common'
import { BulkOperationState, BulkOperationType } from '@sourcegraph/shared/src/graphql-operations'
import { Badge, AlertLink, Link, Alert, Icon, H4, Text } from '@sourcegraph/wildcard'

import { Collapsible } from '../../../../components/Collapsible'
import { Timestamp } from '../../../../components/time/Timestamp'
import { BulkOperationFields } from '../../../../graphql-operations'

import styles from './BulkOperationNode.module.scss'

const OPERATION_TITLES: Record<BulkOperationType, JSX.Element> = {
    COMMENT: (
        <>
            <Icon role="img" aria-hidden={true} className="text-muted" as={CommentOutlineIcon} /> Comment on changesets
        </>
    ),
    DETACH: (
        <>
            <Icon role="img" aria-hidden={true} className="text-muted" as={LinkVariantRemoveIcon} /> Detach changesets
        </>
    ),
    REENQUEUE: (
        <>
            <Icon role="img" aria-hidden={true} className="text-muted" as={SyncIcon} /> Retry changesets
        </>
    ),
    MERGE: (
        <>
            <Icon role="img" aria-hidden={true} className="text-muted" as={SourceBranchIcon} /> Merge changesets
        </>
    ),
    CLOSE: (
        <>
            <Icon role="img" aria-hidden={true} className="text-danger" as={SourceBranchIcon} /> Close changesets
        </>
    ),
    PUBLISH: (
        <>
            <Icon role="img" aria-hidden={true} className="text-muted" as={UploadIcon} /> Publish changesets
        </>
    ),
}

export interface BulkOperationNodeProps {
    node: BulkOperationFields
}

export const BulkOperationNode: React.FunctionComponent<React.PropsWithChildren<BulkOperationNodeProps>> = ({
    node,
}) => (
    <>
        <div
            className={classNames(
                styles.bulkOperationNodeContainer,
                'd-flex justify-content-between align-items-center'
            )}
        >
            <div className={classNames(styles.bulkOperationNodeChangesetCounts, 'text-center')}>
                <Badge variant="secondary" className="mb-2" as="p">
                    {node.changesetCount}
                </Badge>
                <Text className="mb-0">{pluralize('changeset', node.changesetCount)}</Text>
            </div>
            <div className={styles.bulkOperationNodeDivider} />
            <div className="flex-grow-1 ml-3">
                <H4>{OPERATION_TITLES[node.type]}</H4>
                <Text className="mb-0">
                    <Link to={node.initiator.url}>{node.initiator.username}</Link> <Timestamp date={node.createdAt} />
                </Text>
            </div>
            {node.state === BulkOperationState.PROCESSING && (
                <div className={classNames(styles.bulkOperationNodeProgressBar, 'flex-grow-1 ml-3')}>
                    <meter value={node.progress} className="w-100" min={0} max={1} />
                    <Text alignment="center" className="mb-0">
                        {Math.ceil(node.progress * 100)}%
                    </Text>
                </div>
            )}
            {node.state === BulkOperationState.FAILED && (
                <Badge variant="danger" className="text-uppercase">
                    failed
                </Badge>
            )}
            {node.state === BulkOperationState.COMPLETED && (
                <Badge variant="success" className="text-uppercase">
                    complete
                </Badge>
            )}
        </div>
        {node.errors.length > 0 && (
            <div className={classNames(styles.bulkOperationNodeErrors, 'px-4')}>
                <Collapsible
                    titleClassName="flex-grow-1 p-3"
                    title={<H4 className="mb-0">The following errors occured while running this task:</H4>}
                >
                    {node.errors.map((error, index) => (
                        <Alert className="mt-2" key={index} variant="danger">
                            <Text>
                                {error.changeset.__typename === 'HiddenExternalChangeset' ? (
                                    <span className="text-muted">On hidden repository</span>
                                ) : (
                                    <>
                                        <AlertLink to={error.changeset.externalURL?.url ?? ''}>
                                            {error.changeset.title}{' '}
                                            <Icon role="img" aria-hidden={true} as={ExternalLinkIcon} />
                                        </AlertLink>{' '}
                                        on{' '}
                                        <AlertLink to={error.changeset.repository.url}>
                                            repository {error.changeset.repository.name}
                                        </AlertLink>
                                        .
                                    </>
                                )}
                            </Text>
                            {error.error && <ErrorMessage error={'```\n' + error.error + '\n```'} />}
                        </Alert>
                    ))}
                </Collapsible>
            </div>
        )}
    </>
)
