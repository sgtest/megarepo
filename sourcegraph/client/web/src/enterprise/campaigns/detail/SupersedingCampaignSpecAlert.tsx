import React from 'react'
import { Timestamp } from '../../../components/time/Timestamp'
import CreationIcon from 'mdi-react/CreationIcon'
import { Link } from '../../../../../shared/src/components/Link'
import { SupersedingCampaignSpecFields } from '../../../graphql-operations'

export interface SupersedingCampaignSpecAlertProps {
    spec: SupersedingCampaignSpecFields | null
}

export const SupersedingCampaignSpecAlert: React.FunctionComponent<SupersedingCampaignSpecAlertProps> = ({ spec }) => {
    if (!spec) {
        return <></>
    }

    const { applyURL, createdAt } = spec
    return (
        <div className="alert alert-info d-flex align-items-center">
            <div className="d-none d-md-block">
                <CreationIcon className="icon icon-inline mr-2" />
            </div>
            <div className="flex-grow-1">
                A <Link to={applyURL}>newer campaign spec</Link> was uploaded <Timestamp date={createdAt} />.
            </div>
        </div>
    )
}
