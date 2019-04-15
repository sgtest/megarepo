import formatDistanceStrict from 'date-fns/formatDistanceStrict'
import WarningIcon from 'mdi-react/WarningIcon'
import * as React from 'react'
import { Link } from 'react-router-dom'
import { DismissibleAlert } from '../components/DismissibleAlert'

/**
 * A global alert that appears telling the site admin that their license key is about to expire. Even after being dismissed,
 * it reappears every day.
 */
export const LicenseExpirationAlert: React.FunctionComponent<{
    expiresAt: string
    daysLeft: number
    className?: string
}> = ({ expiresAt, daysLeft, className = '' }) => (
    <DismissibleAlert
        partialStorageKey={`licenseExpiring.${daysLeft}`}
        className={`alert alert-warning align-items-center ${className}`}
    >
        <WarningIcon className="icon-inline mr-2 flex-shrink-0" />
        Your Sourcegraph license will expire in {formatDistanceStrict(expiresAt, Date.now())}.&nbsp;
        <Link className="site-alert__link" to="/site-admin/license">
            <span className="underline">Renew now</span>
        </Link>
        &nbsp;or&nbsp;
        <a className="site-alert__link" href="https://about.sourcegraph.com/contact">
            <span className="underline">contact Sourcegraph</span>
        </a>
    </DismissibleAlert>
)
