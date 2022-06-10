import React from 'react'

import classNames from 'classnames'
import { MdiReactIconComponentType } from 'mdi-react'

import { CodeHostIcon } from '@sourcegraph/search-ui'
import { Icon } from '@sourcegraph/wildcard'

import styles from './SearchResultLayout.module.scss'

interface Props {
    children: React.ReactNode
    infoColumn?: React.ReactNode
    iconColumn?: {
        icon: MdiReactIconComponentType
        repoName: string
    }
    className?: string
    isActive?: boolean
}

export const SearchResultLayout: React.FunctionComponent<Props> = ({
    children,
    infoColumn,
    iconColumn,
    className,
    isActive,
}: Props) => (
    <div className={classNames(styles.searchResultLayout, { [styles.active]: isActive })}>
        <div className={styles.iconColumn}>
            {iconColumn !== undefined ? (
                <>
                    <Icon aria-label="File" size="sm" as={iconColumn.icon} />
                    <div className={classNames('mx-1', styles.divider)} />
                    <CodeHostIcon repoName={iconColumn.repoName} />
                </>
            ) : null}
        </div>

        <div className={classNames(styles.contentColumn, className)}>{children}</div>

        <div className={styles.spacer} />

        <div className={styles.infoColumn}>{infoColumn !== undefined ? infoColumn : null}</div>
    </div>
)
