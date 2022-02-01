import classNames from 'classnames'
import * as H from 'history'
import * as React from 'react'
import { Link } from 'react-router-dom'

import { useWildcardTheme } from '../../../hooks/useWildcardTheme'
import { ForwardReferenceComponent } from '../../../types'
import type { LinkProps } from '../Link'

import styles from './AnchorLink.module.scss'

export type AnchorLinkProps = LinkProps

export const AnchorLink = React.forwardRef(({ to, as: Component, children, className, ...rest }, reference) => {
    const { isBranded } = useWildcardTheme()

    const commonProps = {
        ref: reference,
        className: classNames(isBranded && styles.anchorLink, className),
    }

    if (!Component) {
        return (
            <a href={to && typeof to !== 'string' ? H.createPath(to) : to} {...rest} {...commonProps}>
                {children}
            </a>
        )
    }

    return (
        <Component to={to} {...rest} {...commonProps}>
            {children}
        </Component>
    )
}) as ForwardReferenceComponent<Link<unknown>, AnchorLinkProps>
