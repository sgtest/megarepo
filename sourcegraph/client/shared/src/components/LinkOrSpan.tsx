import * as React from 'react'

import { ForwardReferenceComponent, Link } from '@sourcegraph/wildcard'

type Props = React.PropsWithChildren<
    {
        to: string | undefined | null
        children?: React.ReactNode
    } & React.AnchorHTMLAttributes<HTMLAnchorElement>
>

/**
 * The LinkOrSpan component renders a <Link> if the "to" property is a non-empty string; otherwise it renders the
 * text in a <span> (with no link).
 */
export const LinkOrSpan = React.forwardRef(({ to, className = '', children, ...otherProps }: Props, reference) => {
    if (to) {
        return (
            <Link ref={reference} to={to} className={className} {...otherProps}>
                {children}
            </Link>
        )
    }

    return (
        <span ref={reference} className={className} {...otherProps}>
            {children}
        </span>
    )
}) as ForwardReferenceComponent<typeof Link, Props>
