import React, { FunctionComponent } from 'react'

import { Link, H3, Code } from '@sourcegraph/wildcard'

import { LockfileIndexFields } from '../../../../graphql-operations'

import styles from './CodeIntelLockfileIndexNode.module.scss'

export interface CodeIntelLockfileNodeProps {
    node: LockfileIndexFields
}

export const CodeIntelLockfileNode: FunctionComponent<React.PropsWithChildren<CodeIntelLockfileNodeProps>> = ({
    node,
}) => (
    <>
        <span className={styles.separator} />

        <div className="d-flex flex-column">
            <div className="m-0">
                <H3 className="m-0 d-block d-md-inline">
                    <Link to={node.repository.url}>{node.repository.name}</Link>
                </H3>
            </div>

            <div>
                <span className="mr-2 d-block d-mdinline-block">
                    Lockfile <Code>{node.lockfile}</Code> indexed at commit{' '}
                    <Link to={node.commit.url}>
                        <Code>{node.commit.abbreviatedOID}</Code>
                    </Link>
                    . Dependency graph fidelity: {node.fidelity}.
                </span>
            </div>
        </div>
    </>
)
