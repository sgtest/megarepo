import React, { PropsWithChildren, ReactNode, RefObject } from 'react'

import classNames from 'classnames'

import styles from './FormGroup.module.scss'

interface FormGroupProps {
    /** Name attr value for root fieldset element. */
    name: string
    /** Title on top of group. */
    title: string
    /** Subtitle of group. */
    subtitle?: ReactNode
    /** Error message for field group. */
    error?: string
    /** Description text, renders below of content inputs of group. */
    description?: ReactNode
    /** Custom class name for root fieldset element. */
    className?: string
    /** Custom class name for label element of the group. */
    labelClassName?: string
    /** Custom class name for div children wrapper element. */
    contentClassName?: string
    /** Reference to root fieldset element.*/
    innerRef?: RefObject<HTMLFieldSetElement>
}

/** Displays fieldset (group) of fields for code insight creation form with error message. */
export const FormGroup: React.FunctionComponent<React.PropsWithChildren<PropsWithChildren<FormGroupProps>>> = props => {
    const {
        innerRef,
        className,
        labelClassName,
        contentClassName,
        name,
        title,
        subtitle,
        children,
        description,
        error,
    } = props

    return (
        <fieldset ref={innerRef} name={name} className={className}>
            <legend className="d-flex flex-column mb-3">
                <div className={classNames(labelClassName, styles.label)}>{title}</div>

                {/* Since safari doesn't support flex column on legend element we have to set d-block*/}
                {/* explicitly */}
                {subtitle && <span className={classNames('d-block text-muted', styles.description)}>{subtitle}</span>}
                {error && (
                    <small role="alert" className="d-block text-danger">
                        {error}
                    </small>
                )}
            </legend>

            <div className={contentClassName}>{children}</div>

            {description && <small className="d-block mt-3 text-muted">{description}</small>}
        </fieldset>
    )
}
