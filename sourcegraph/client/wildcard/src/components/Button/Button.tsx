import classNames from 'classnames'
import React from 'react'

import { ForwardReferenceComponent } from '../../types'

import { BUTTON_VARIANTS, BUTTON_SIZES } from './constants'
import { getButtonSize, getButtonStyle } from './utils'

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
    /**
     * The variant style of the button. Defaults to `primary`
     */
    variant?: typeof BUTTON_VARIANTS[number]
    /**
     * Allows modifying the size of the button. Supports larger or smaller variants.
     */
    size?: typeof BUTTON_SIZES[number]
    /**
     * Modifies the button style to have a transparent/light background and a more pronounced outline.
     */
    outline?: boolean
}

/**
 * Simple button.
 *
 * Style can be configured using different button `variant`s.
 *
 * Buttons should be used to allow users to trigger specific actions on the page.
 * Always be mindful of how intent is signalled to the user when using buttons. We should consider the correct button `variant` for each action.
 *
 * Some examples:
 * - The main action a user should take on the page should usually be styled with the `primary` variant.
 * - Other additional actions on the page should usually be styled with the `secondary` variant.
 * - A destructive 'delete' action should be styled with the `danger` variant.
 *
 * Tips:
 * - Avoid using button styling for links where possible. Buttons should typically trigger an action, links should navigate to places.
 */
export const Button = React.forwardRef(
    (
        {
            children,
            as: Component = 'button',
            // Use default type="button" only for the `button` element.
            type = Component === 'button' ? 'button' : undefined,
            variant,
            size,
            outline,
            className,
            ...attributes
        },
        reference
    ) => (
        <Component
            ref={reference}
            className={classNames(
                'btn',
                variant && getButtonStyle({ variant, outline }),
                size && getButtonSize({ size }),
                className
            )}
            type={type}
            {...attributes}
        >
            {children}
        </Component>
    )
) as ForwardReferenceComponent<'button', ButtonProps>
