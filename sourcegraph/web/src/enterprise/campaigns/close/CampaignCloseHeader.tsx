import React from 'react'
import { ChangesetFields } from '../../../graphql-operations'

export interface CampaignCloseHeaderProps {
    nodes: ChangesetFields[]
    totalCount?: number | null
}

export const CampaignCloseHeader: React.FunctionComponent<CampaignCloseHeaderProps> = ({ nodes, totalCount }) => (
    <>
        <div className="campaign-close-header__title mb-2">
            <strong>
                Displaying {nodes.length}
                {totalCount && <> of {totalCount}</>} changesets
            </strong>
        </div>
        <span />
        <h5 className="text-uppercase text-center text-nowrap text-muted">Action</h5>
        <h5 className="text-uppercase text-nowrap text-muted">Changeset information</h5>
        <h5 className="text-uppercase text-center text-nowrap text-muted">Check state</h5>
        <h5 className="text-uppercase text-center text-nowrap text-muted">Review state</h5>
        <h5 className="text-uppercase text-right text-nowrap text-muted">Changes</h5>
    </>
)
