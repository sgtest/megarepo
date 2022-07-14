import { FunctionComponent } from 'react'

import { mdiChevronRight } from '@mdi/js'
import classNames from 'classnames'

import { Link, H3, Icon } from '@sourcegraph/wildcard'

import { LsifIndexFields } from '../../../../graphql-operations'
import { CodeIntelState } from '../../shared/components/CodeIntelState'
import { CodeIntelUploadOrIndexCommit } from '../../shared/components/CodeIntelUploadOrIndexCommit'
import { CodeIntelUploadOrIndexCommitTags } from '../../shared/components/CodeIntelUploadOrIndexCommitTags'
import { CodeIntelUploadOrIndexRepository } from '../../shared/components/CodeIntelUploadOrIndexerRepository'
import { CodeIntelUploadOrIndexIndexer } from '../../shared/components/CodeIntelUploadOrIndexIndexer'
import { CodeIntelUploadOrIndexLastActivity } from '../../shared/components/CodeIntelUploadOrIndexLastActivity'
import { CodeIntelUploadOrIndexRoot } from '../../shared/components/CodeIntelUploadOrIndexRoot'

import styles from './CodeIntelIndexNode.module.scss'

export interface CodeIntelIndexNodeProps {
    node: LsifIndexFields
    now?: () => Date
}

export const CodeIntelIndexNode: FunctionComponent<React.PropsWithChildren<CodeIntelIndexNodeProps>> = ({
    node,
    now,
}) => (
    <>
        <span className={styles.separator} />

        <div className={classNames(styles.information, 'd-flex flex-column')}>
            <div className="m-0">
                <H3 className="m-0 d-block d-md-inline">
                    <CodeIntelUploadOrIndexRepository node={node} />
                </H3>
            </div>

            <div>
                <span className="mr-2 d-block d-mdinline-block">
                    Directory <CodeIntelUploadOrIndexRoot node={node} /> indexed at commit{' '}
                    <CodeIntelUploadOrIndexCommit node={node} />
                    {node.tags.length > 0 && (
                        <>
                            , <CodeIntelUploadOrIndexCommitTags tags={node.tags} />,
                        </>
                    )}{' '}
                    by <CodeIntelUploadOrIndexIndexer node={node} />
                </span>

                <small className="text-mute">
                    <CodeIntelUploadOrIndexLastActivity node={{ ...node, uploadedAt: null }} now={now} />
                </small>
            </div>
        </div>

        <span className={classNames(styles.state, 'd-none d-md-inline')}>
            <CodeIntelState node={node} className="d-flex flex-column align-items-center" />
        </span>
        <span>
            <Link to={`./indexes/${node.id}`}>
                <Icon svgPath={mdiChevronRight} inline={false} aria-label="View more information" />
            </Link>
        </span>
    </>
)
