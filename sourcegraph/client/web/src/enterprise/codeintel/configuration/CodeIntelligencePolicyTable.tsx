import classNames from 'classnames'
import * as H from 'history'
import PencilIcon from 'mdi-react/PencilIcon'
import TrashIcon from 'mdi-react/TrashIcon'
import React, { FunctionComponent } from 'react'

import { Tooltip } from '@sourcegraph/branded/src/components/tooltip/Tooltip'
import { GitObjectType } from '@sourcegraph/shared/src/graphql/schema'
import { Button } from '@sourcegraph/wildcard'

import { CodeIntelligenceConfigurationPolicyFields } from '../../../graphql-operations'

import styles from './CodeIntelligencePolicyTable.module.scss'
import { IndexingPolicyDescription } from './IndexingPolicyDescription'
import { RetentionPolicyDescription } from './RetentionPolicyDescription'

export interface CodeIntelligencePolicyTableProps {
    indexingEnabled: boolean
    disabled: boolean
    policies: CodeIntelligenceConfigurationPolicyFields[]
    onDeletePolicy?: (id: string, name: string) => Promise<void>
    history: H.History
}

export const CodeIntelligencePolicyTable: FunctionComponent<CodeIntelligencePolicyTableProps> = ({
    indexingEnabled,
    disabled,
    policies,
    onDeletePolicy,
    history,
}) => (
    <div className={styles.grid}>
        {policies.map(policy => (
            <React.Fragment key={policy.id}>
                <span className={styles.separator} />

                <div className={classNames(styles.name, 'd-flex flex-column')}>
                    <div className="m-0">
                        <h3 className="m-0 d-block d-md-inline">{policy.name}</h3>
                    </div>

                    <div>
                        <div className="mr-2 d-block d-mdinline-block">
                            Applied to{' '}
                            {policy.type === GitObjectType.GIT_COMMIT
                                ? 'commits'
                                : policy.type === GitObjectType.GIT_TAG
                                ? 'tags'
                                : policy.type === GitObjectType.GIT_TREE
                                ? 'branches'
                                : ''}{' '}
                            matching <span className="text-monospace">{policy.pattern}</span>
                            {policy.repositoryPatterns && (
                                <>
                                    {' '}
                                    in repositories matching{' '}
                                    {policy.repositoryPatterns.map((pattern, index) => (
                                        <>
                                            {index !== 0 &&
                                                (index === (policy.repositoryPatterns || []).length - 1 ? (
                                                    <>, or </>
                                                ) : (
                                                    <>, </>
                                                ))}
                                            <span key={pattern} className="text-monospace">
                                                {pattern}
                                            </span>
                                        </>
                                    ))}
                                </>
                            )}
                        </div>

                        <div>
                            {indexingEnabled && !policy.retentionEnabled && !policy.indexingEnabled ? (
                                <p className="text-muted mt-2">Data retention and auto-indexing disabled.</p>
                            ) : (
                                <>
                                    <p className="mt-2">
                                        <RetentionPolicyDescription policy={policy} />
                                    </p>
                                    {indexingEnabled && (
                                        <p className="mt-2">
                                            <IndexingPolicyDescription policy={policy} />
                                        </p>
                                    )}
                                </>
                            )}
                        </div>
                    </div>
                </div>

                <span className={classNames(styles.button, 'd-none d-md-inline')}>
                    {onDeletePolicy && (
                        <Button
                            onClick={() => history.push(`./configuration/${policy.id}`)}
                            className="p-0"
                            disabled={disabled}
                        >
                            <Tooltip />
                            <PencilIcon className="icon-inline" data-tooltip="Edit the policy" />
                        </Button>
                    )}
                </span>
                <span className={classNames(styles.button, 'd-none d-md-inline')}>
                    {onDeletePolicy && !policy.protected && (
                        <Button
                            onClick={() => onDeletePolicy(policy.id, policy.name)}
                            className="ml-2 p-0"
                            disabled={disabled}
                        >
                            <Tooltip />
                            <TrashIcon className="icon-inline text-danger" data-tooltip="Delete the policy" />
                        </Button>
                    )}
                </span>
            </React.Fragment>
        ))}
    </div>
)
