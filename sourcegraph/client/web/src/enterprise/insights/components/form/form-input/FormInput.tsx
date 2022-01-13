import classNames from 'classnames'
import React, { useRef, forwardRef, InputHTMLAttributes, ReactNode } from 'react'
import { useMergeRefs } from 'use-callback-ref'

import { LoaderInput } from '@sourcegraph/branded/src/components/LoaderInput'
import { useAutoFocus, ForwardReferenceComponent } from '@sourcegraph/wildcard'

import styles from './FormInput.module.scss'

interface FormInputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'title'> {
    /** Title of input. */
    title?: ReactNode

    subtitle?: ReactNode
    /** Description block for field. */
    description?: ReactNode
    /** Custom class name for root label element. */
    className?: string
    /** Error massage for input. */
    error?: string
    /** Prop to control error input element state. */
    errorInputState?: boolean
    /** Valid sign to show valid state on input. */
    valid?: boolean
    /** Turn on loading state (visually this is an input with loader) */
    loading?: boolean
    /** Turn on or turn off autofocus for input. */
    autofocus?: boolean
    /** Custom class name for input element. */
    inputClassName?: string
    /** Input icon (symbol) which render right after the input element. */
    inputSymbol?: ReactNode
}

/**
 * Displays the input with description, error message, visual invalid and valid states.
 */
const FormInput = forwardRef((props, reference) => {
    const {
        as: Component = 'input',
        type = 'text',
        title,
        subtitle,
        description,
        className,
        inputClassName,
        inputSymbol,
        valid,
        error,
        loading = false,
        errorInputState,
        autoFocus,
        ...otherProps
    } = props

    const localReference = useRef<HTMLInputElement>(null)
    const mergedReference = useMergeRefs([localReference, reference])

    useAutoFocus({ autoFocus, reference: localReference })

    return (
        <label className={classNames('w-100', className)}>
            {title && <div className="mb-2">{title}</div>}

            {subtitle}

            <LoaderInput className="d-flex" loading={loading}>
                <Component
                    type={type}
                    className={classNames(styles.input, inputClassName, 'form-control', 'with-invalid-icon', {
                        'is-valid': valid,
                        'is-invalid': !!error || errorInputState,
                    })}
                    {...otherProps}
                    ref={mergedReference}
                    autoFocus={autoFocus}
                />

                {inputSymbol}
            </LoaderInput>

            {error && (
                <small role="alert" className="text-danger form-text">
                    {error}
                </small>
            )}
            {!error && description && (
                <small className={classNames('text-muted', 'form-text', styles.description)}>{description}</small>
            )}
        </label>
    )
}) as ForwardReferenceComponent<'input', FormInputProps>

FormInput.displayName = 'FormInput'

export { FormInput }
