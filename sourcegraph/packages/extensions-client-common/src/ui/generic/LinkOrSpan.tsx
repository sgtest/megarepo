import * as React from 'react'
import { Link } from 'react-router-dom'

/**
 * The LinkOrSpan component renders a <Link> (from react-router-dom) if the "to" property is a non-empty string;
 * otherwise it renders the text in a <span> (with no link).
 */
export const LinkOrSpan: React.SFC<
    {
        to: string | undefined | null
        children?: React.ReactNode
    } & React.AnchorHTMLAttributes<HTMLAnchorElement>
> = ({ to, className = '', children, ...otherProps }) =>
    to ? (
        <Link to={to} className={className} {...otherProps}>
            {children}
        </Link>
    ) : (
        <span className={className} {...otherProps}>
            {children}
        </span>
    )
