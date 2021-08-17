import classNames from 'classnames'
import React, { useCallback, useRef } from 'react'
import { useMergeRefs } from 'use-callback-ref'

import { Form } from '@sourcegraph/branded/src/components/Form'
import { useAutoFocus } from '@sourcegraph/wildcard'

import { FilterControl, FilteredConnectionFilter, FilteredConnectionFilterValue } from '../FilterControl'

export interface ConnectionFormProps {
    /** Hides the filter input field. */
    hideSearch?: boolean

    /** CSS class name for the <input> element */
    inputClassName?: string

    /** Placeholder text for the <input> element */
    inputPlaceholder?: string

    /** Value of the <input> element */
    inputValue?: string

    /** Called when the <input> element value changes */
    onInputChange?: React.ChangeEventHandler<HTMLInputElement>

    /** Autofocuses the filter input field. */
    autoFocus?: boolean

    /**
     * Filters to display next to the filter input field.
     *
     * Filters are mutually exclusive.
     */
    filters?: FilteredConnectionFilter[]

    onValueSelect?: (filter: FilteredConnectionFilter, value: FilteredConnectionFilterValue) => void

    /** An element rendered as a sibling of the filters. */
    additionalFilterElement?: React.ReactElement

    values?: Map<string, FilteredConnectionFilterValue>
}

/**
 * FilteredConnection form input.
 * Supports <input> for querying and <select>/<radio> controls for filtering
 */
export const ConnectionForm = React.forwardRef<HTMLInputElement, ConnectionFormProps>(
    (
        {
            hideSearch,
            inputClassName,
            inputPlaceholder,
            inputValue,
            onInputChange,
            autoFocus,
            filters,
            onValueSelect,
            additionalFilterElement,
            values,
        },
        reference
    ) => {
        const localReference = useRef<HTMLInputElement>(null)
        const mergedReference = useMergeRefs([localReference, reference])
        const handleSubmit = useCallback<React.FormEventHandler<HTMLFormElement>>(event => {
            // Do nothing. The <input onChange> handler will pick up any changes shortly.
            event.preventDefault()
        }, [])

        useAutoFocus({ autoFocus, reference: localReference })

        return (
            <Form
                className="w-100 d-inline-flex justify-content-between flex-row filtered-connection__form"
                onSubmit={handleSubmit}
            >
                {filters && onValueSelect && values && (
                    <FilterControl filters={filters} onValueSelect={onValueSelect} values={values}>
                        {additionalFilterElement}
                    </FilterControl>
                )}
                {!hideSearch && (
                    <input
                        className={classNames('form-control', inputClassName)}
                        type="search"
                        placeholder={inputPlaceholder}
                        name="query"
                        value={inputValue}
                        onChange={onInputChange}
                        autoFocus={autoFocus}
                        autoComplete="off"
                        autoCorrect="off"
                        autoCapitalize="off"
                        ref={mergedReference}
                        spellCheck={false}
                    />
                )}
            </Form>
        )
    }
)
