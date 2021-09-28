import classNames from 'classnames'
import React from 'react'

import styles from './ConnectionList.module.scss'

interface ConnectionListProps {
    /** list HTML element type. Default is <ul>. */
    as?: 'ul' | 'table' | 'div'

    /** CSS class name for the list element (<ul>, <table>, or <div>). */
    className?: string

    compact?: boolean
}

/**
 * Render a list of FilteredConnection nodes.
 * Can be configured to render as different elements to support alternative representations of data such as through the <table> element.
 */
export const ConnectionList: React.FunctionComponent<ConnectionListProps> = ({
    as: ListComponent = 'ul',
    className,
    children,
    compact,
}) => (
    <ListComponent
        className={classNames(styles.normal, compact && styles.compact, className)}
        data-testid="filtered-connection-nodes"
    >
        {children}
    </ListComponent>
)
