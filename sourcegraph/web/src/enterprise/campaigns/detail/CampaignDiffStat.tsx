import React, { useMemo } from 'react'
import * as GQL from '../../../../../shared/src/graphql/schema'
import { DiffStat } from '../../../components/diff/DiffStat'

export interface CampaignDiffstatProps {
    campaign?: {
        diffStat: Pick<GQL.IDiffStat, 'added' | 'changed' | 'deleted'>
    }
    patchSet?: {
        diffStat: Pick<GQL.IDiffStat, 'added' | 'changed' | 'deleted'>
    }

    className?: string
}

/**
 * Total diff stat of a campaign or patchset, including all changesets and patches
 */
export const CampaignDiffStat: React.FunctionComponent<CampaignDiffstatProps> = ({ campaign, patchSet, className }) => {
    const { added, changed, deleted } = useMemo(() => (campaign ? campaign.diffStat : patchSet!.diffStat), [
        campaign,
        patchSet,
    ])

    if (added + changed + deleted === 0) {
        return <></>
    }

    return (
        <div className={className}>
            <DiffStat expandedCounts={true} added={added} changed={changed} deleted={deleted} />
        </div>
    )
}
