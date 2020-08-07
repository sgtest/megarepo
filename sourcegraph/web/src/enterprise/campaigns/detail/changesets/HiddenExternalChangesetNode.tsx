import React from 'react'
import { ChangesetStateIcon } from './ChangesetStateIcon'
import { ChangesetLastSynced } from './ChangesetLastSynced'
import { ChangesetFields } from '../../../../graphql-operations'

export interface HiddenExternalChangesetNodeProps {
    node: Pick<ChangesetFields, 'id' | 'nextSyncAt' | 'updatedAt' | 'externalState'>
}

export const HiddenExternalChangesetNode: React.FunctionComponent<HiddenExternalChangesetNodeProps> = ({ node }) => (
    <li className="list-group-item test-changeset-node">
        <div className="changeset-node__content changeset-node__content--no-collapse flex-fill">
            <div className="d-flex align-items-start m-1 ml-2">
                <div className="changeset-node__content flex-fill">
                    <div className="d-flex flex-column">
                        <div className="m-0 mb-2">
                            <h3 className="m-0 d-inline">
                                {node.externalState && <ChangesetStateIcon externalState={node.externalState} />}
                                <span className="text-muted">Changeset in a private repository</span>
                            </h3>
                        </div>
                        <div>
                            <ChangesetLastSynced changeset={node} viewerCanAdminister={false} />
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </li>
)
