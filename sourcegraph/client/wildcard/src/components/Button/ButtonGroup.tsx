import classNames from 'classnames'
import React from 'react'

import { ForwardReferenceComponent } from '../..'

import styles from './Button.module.scss'
import { BUTTON_GROUP_DIRECTION } from './constants'

export interface ButtonGroupProps {
    /**
     * Used to change the element that is rendered, default to div
     */
    as?: React.ElementType
    /**
     * Defines the orientaion contained button elements. defaults to horizontal
     */
    direction?: typeof BUTTON_GROUP_DIRECTION[number]
}

export const ButtonGroup = React.forwardRef(
    ({ as: Component = 'div', children, className, direction, ...attributes }, reference) => (
        <Component
            ref={reference}
            role="group"
            className={classNames(styles.btnGroup, direction === 'vertical' && styles.btnGroupVertical, className)}
            {...attributes}
        >
            {children}
        </Component>
    )
) as ForwardReferenceComponent<'div', ButtonGroupProps>
