import React from 'react'

import { LinkWithIcon } from '../../../../components/LinkWithIcon'
import { CodeInsightsIcon } from '../../../../insights/Icons'

export const InsightsNavItem: React.FunctionComponent = () => (
    <LinkWithIcon
        to="/insights"
        text="Insights"
        icon={CodeInsightsIcon}
        className="nav-link text-decoration-none"
        activeClassName="active"
    />
)
