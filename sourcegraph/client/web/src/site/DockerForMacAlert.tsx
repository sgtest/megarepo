import classNames from 'classnames'
import React from 'react'

import { Link } from '@sourcegraph/wildcard'

import { DismissibleAlert } from '../components/DismissibleAlert'

import styles from './DockerForMacAlert.module.scss'

/**
 * A global alert telling all users that due to Docker for Mac, site performance
 * will be degraded.
 */
export const DockerForMacAlert: React.FunctionComponent<{ className?: string }> = ({ className }) => (
    <DismissibleAlert
        partialStorageKey="DockerForMac"
        className={classNames('alert-warning docker-for-mac-alert d-flex align-items-center', className)}
    >
        <span className={styles.left}>
            It looks like you're using Docker for Mac. Due to known issues related to Docker for Mac's file system
            access, search performance and cloning repositories on Sourcegraph will be much slower.
        </span>
        <span className={styles.right}>
            <Link to="/help/admin">Run Sourcegraph on a different platform or deploy it to a server</Link> for much
            faster performance.
        </span>
    </DismissibleAlert>
)
