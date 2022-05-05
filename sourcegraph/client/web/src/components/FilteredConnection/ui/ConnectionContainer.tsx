import React from 'react'

import classNames from 'classnames'

import styles from './ConnectionContainer.module.scss'

interface ConnectionContainerProps {
    className?: string
    compact?: boolean
}

/**
 * A styled FilteredConnection container.
 * This component should wrap other FilteredConnection components.
 * Use `compact` to modify styling across FilteredConnection.
 */
export const ConnectionContainer: React.FunctionComponent<React.PropsWithChildren<ConnectionContainerProps>> = ({
    children,
    className,
    compact,
}) => (
    <div
        data-testid="filtered-connection"
        className={classNames(styles.normal, !compact && styles.noncompact, className)}
    >
        {children}
    </div>
)
