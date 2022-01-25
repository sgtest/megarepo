import classNames from 'classnames'
import React from 'react'

import { ForwardReferenceComponent } from '../../..'

import styles from './CardSubtitle.module.scss'

interface CardSubtitleProps {}

export const CardSubtitle = React.forwardRef(
    ({ as: Component = 'div', children, className, ...attributes }, reference) => (
        <Component ref={reference} className={classNames(styles.cardSubtitle, className)} {...attributes}>
            {children}
        </Component>
    )
) as ForwardReferenceComponent<'div', CardSubtitleProps>
