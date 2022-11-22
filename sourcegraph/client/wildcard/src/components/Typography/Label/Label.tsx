import React, { MouseEvent } from 'react'

import classNames from 'classnames'
import { useMergeRefs } from 'use-callback-ref'

import { ForwardReferenceComponent } from '../../../types'
import { TypographyProps } from '../utils'

import { getLabelClassName } from './utils'

interface LabelProps extends React.HTMLAttributes<HTMLLabelElement>, TypographyProps {
    size?: 'small' | 'base'
    weight?: 'regular' | 'medium' | 'bold'
    isUnderline?: boolean
    isUppercase?: boolean
}

/**
 * Wildcard implementation of the label element.
 *
 * Use this Label component for non-standard form controls such as contenteditable inputs.
 * Make sure you connect the input element and label with an aria-labelledby attribute.
 * Otherwise, label native behaviour click-to-focus won't work.
 */
export const Label = React.forwardRef((props, reference) => {
    const {
        children,
        as: Component = 'label',
        size,
        weight,
        alignment,
        mode,
        isUnderline,
        isUppercase,
        className,
        onClick,
        ...rest
    } = props

    const mergedRef = useMergeRefs([reference])

    // Listen to all clicks on the label element in order to improve click-to-focus logic
    // for contenteditable="true". By default, label element's native behavior (click to focus the first input element)
    // doesn't work with contenteditable elements.
    // Since we use contenteditable inputs (the CodeMirror search box) and labels together in some
    // consumers, we need to support this behavior manually for contenteditable elements.
    const handleClick = (event: MouseEvent<HTMLLabelElement>): void => {
        const forAttribute = mergedRef.current?.getAttribute('for')

        if (forAttribute) {
            onClick?.(event)
            return
        }

        // Resolve label's labellable control with aria-labelledby
        // See https://github.com/sourcegraph/sourcegraph/pull/44676#pullrequestreview-1188749383
        document.querySelector<HTMLElement>(`[aria-labelledby="${event.currentTarget.id}"]`)?.focus()
        onClick?.(event)
    }

    return (
        <Component
            ref={mergedRef}
            className={classNames(
                getLabelClassName({ isUppercase, isUnderline, alignment, weight, size, mode }),
                className
            )}
            onClick={handleClick}
            {...rest}
        >
            {children}
        </Component>
    )
}) as ForwardReferenceComponent<'label', LabelProps>
