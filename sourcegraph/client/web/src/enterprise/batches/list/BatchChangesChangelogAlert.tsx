import React from 'react'

import classNames from 'classnames'

import { CardBody, Card, H3, H4 } from '@sourcegraph/wildcard'

import { DismissibleAlert } from '../../../components/DismissibleAlert'

import styles from './BatchChangesListIntro.module.scss'

export const BatchChangesChangelogAlert: React.FunctionComponent<React.PropsWithChildren<unknown>> = () => (
    <DismissibleAlert
        className={styles.batchChangesListIntroAlert}
        partialStorageKey="batch-changes-list-intro-changelog-3.42"
    >
        <Card className={classNames(styles.batchChangesListIntroCard, 'h-100')}>
            <CardBody>
                <H4 as={H3}>Batch Changes updates in version 3.42</H4>
                <ul className="mb-0 pl-3">
                    <li>Mounted files can be cached when executing a batch spec.</li>
                    <li>Improved keyboard navigation for server-side execution flow.</li>
                </ul>
            </CardBody>
        </Card>
    </DismissibleAlert>
)
