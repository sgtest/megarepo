import React from 'react'

import classNames from 'classnames'

import { useWildcardTheme } from '../../hooks/useWildcardTheme'
import { ForwardReferenceComponent } from '../../types'

import { BUTTON_VARIANTS, BUTTON_SIZES, BUTTON_DISPLAY } from './constants'
import { getButtonSize, getButtonStyle, getButtonDisplay } from './utils'

import styles from './Button.module.scss'

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
     * Allows modifying the display property of the button. Supports inline-block or block variants.
     */
    display?: typeof BUTTON_DISPLAY[number]
    /**
     * Modifies the button style to have a transparent/light background and a more pronounced outline.
     */
    outline?: boolean
    /**
     * A tooltip to display when the user hovers the button.
     */
    ['data-tooltip']?: string
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
// eslint-disable-next-line react/display-name
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
            disabled,
            display,
            ...attributes
        },
        reference
    ) => {
        const { isBranded } = useWildcardTheme()

        const brandedButtonClassname = classNames(
            styles.btn,
            variant && getButtonStyle({ variant, outline }),
            display && getButtonDisplay({ display }),
            size && getButtonSize({ size })
        )

        return (
            <Component
                ref={reference}
                className={classNames(isBranded && brandedButtonClassname, className)}
                type={type}
                disabled={disabled}
                {...attributes}
            >
                {children}
            </Component>
        )
    }
) as ForwardReferenceComponent<'button', ButtonProps>

Button.displayName = 'Button'
