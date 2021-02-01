import React from 'react'
import { LinkWithIcon } from '../components/LinkWithIcon'
import { InsightsIcon } from './icon'

export const InsightsNavItem: React.FunctionComponent = () => (
    <LinkWithIcon
        to="/insights"
        text="Insights"
        icon={InsightsIcon}
        className="nav-link btn btn-link px-1 text-decoration-none"
        activeClassName="active"
    />
)
