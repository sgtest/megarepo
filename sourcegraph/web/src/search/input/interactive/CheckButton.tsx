import CheckIcon from 'mdi-react/CheckIcon'
import React from 'react'
import classNames from 'classnames'

/**
 * A checkmark button for the filter input. It must be wrapped in a form whose onSubmit
 * handler performs a new search with the filter value.
 */
export const CheckButton: React.FunctionComponent<{ className?: string }> = ({ className }) => (
    <div className="search-button d-flex">
        <button
            className={classNames('btn', 'btn-primary', className, 'e2e-confirm-filter-button')}
            type="submit"
            aria-label="Confirm filter"
            data-tooltip="Confirm filter"
        >
            <CheckIcon className="icon-inline" aria-hidden="true" />
        </button>
    </div>
)
