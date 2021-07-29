import classNames from 'classnames'
import React from 'react'

interface ShowMoreProps {
    className?: string
    onClick: () => void
}

/**
 * FilteredConnection styled Button to support fetching more results
 */
export const ShowMoreButton: React.FunctionComponent<ShowMoreProps> = ({ className, onClick }) => (
    <button
        type="button"
        className={classNames('btn btn-sm filtered-connection__show-more btn-link', className)}
        onClick={onClick}
    >
        Show more
    </button>
)
