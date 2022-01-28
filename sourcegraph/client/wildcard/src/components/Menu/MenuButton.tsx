import { MenuButton as ReachMenuButton } from '@reach/menu-button'
import React from 'react'

import { ForwardReferenceComponent } from '../../types'
import { Button, ButtonProps } from '../Button'
import { PopoverTrigger } from '../Popover'

export type MenuButtonProps = Omit<ButtonProps, 'as'>

/**
 * Wraps a styled Wildcard `<Button />` component that can
 * toggle the opening and closing of a dropdown menu.
 *
 * @see — Docs https://reach.tech/menu-button#menubutton
 */
export const MenuButton = React.forwardRef(({ children, ...props }, reference) => (
    <ReachMenuButton ref={reference} as={PopoverTriggerButton} {...props}>
        {children}
    </ReachMenuButton>
)) as ForwardReferenceComponent<'button', MenuButtonProps>

const PopoverTriggerButton = React.forwardRef((props, reference) => (
    <PopoverTrigger ref={reference} as={Button} {...props} />
)) as ForwardReferenceComponent<'button', ButtonProps>
