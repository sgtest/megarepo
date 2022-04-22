import { FunctionComponent } from 'react'

import classNames from 'classnames'
import ChevronRightIcon from 'mdi-react/ChevronRightIcon'

import { Link } from '@sourcegraph/wildcard'

import { LsifUploadFields } from '../../../../graphql-operations'
import { CodeIntelState } from '../../shared/components/CodeIntelState'
import { CodeIntelUploadOrIndexCommit } from '../../shared/components/CodeIntelUploadOrIndexCommit'
import { CodeIntelUploadOrIndexRepository } from '../../shared/components/CodeIntelUploadOrIndexerRepository'
import { CodeIntelUploadOrIndexIndexer } from '../../shared/components/CodeIntelUploadOrIndexIndexer'
import { CodeIntelUploadOrIndexRoot } from '../../shared/components/CodeIntelUploadOrIndexRoot'

import styles from './DependencyOrDependentNode.module.scss'

export interface DependencyOrDependentNodeProps {
    node: LsifUploadFields
    now?: () => Date
}

export const DependencyOrDependentNode: FunctionComponent<DependencyOrDependentNodeProps> = ({ node }) => (
    <>
        <span className={styles.separator} />

        <div className={classNames(styles.information, 'd-flex flex-column')}>
            <div className="m-0">
                <h3 className="m-0 d-block d-md-inline">
                    <CodeIntelUploadOrIndexRepository node={node} />
                </h3>
            </div>

            <div>
                <span className="mr-2 d-block d-mdinline-block">
                    Directory <CodeIntelUploadOrIndexRoot node={node} /> indexed at commit{' '}
                    <CodeIntelUploadOrIndexCommit node={node} /> by <CodeIntelUploadOrIndexIndexer node={node} />
                </span>
            </div>
        </div>

        <span className={classNames(styles.state, 'd-none d-md-inline')}>
            <CodeIntelState node={node} className="d-flex flex-column align-items-center" />
        </span>
        <span>
            <Link to={`./${node.id}`}>
                <ChevronRightIcon />
            </Link>
        </span>
    </>
)
