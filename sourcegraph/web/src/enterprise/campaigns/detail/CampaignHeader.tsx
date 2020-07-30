import React from 'react'
import { CampaignsIcon } from '../icons'
import classNames from 'classnames'

interface Props {
    className?: string
}

/**
 * The header bar for campaigns pages.
 */
export const CampaignHeader: React.FunctionComponent<Props> = ({ className }) => (
    <h1 className={classNames(className)}>
        <CampaignsIcon className="icon-inline mr-2" />
        Campaigns
        <sup>
            <span className="ml-2 badge badge-primary text-uppercase">Beta</span>
        </sup>
    </h1>
)
