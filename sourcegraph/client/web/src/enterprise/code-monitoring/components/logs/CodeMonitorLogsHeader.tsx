import React from 'react'

import classNames from 'classnames'

import styles from './CodeMonitorLogsHeader.module.scss'

export const CodeMonitorLogsHeader: React.FunctionComponent<React.PropsWithChildren<{}>> = () => (
    <>
        <h5 className={classNames(styles.nameColumn, 'text-uppercase text-nowrap')}>Monitor name</h5>
        <h5 className="text-uppercase text-nowrap">Last run</h5>
    </>
)
