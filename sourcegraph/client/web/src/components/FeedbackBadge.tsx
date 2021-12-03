import classNames from 'classnames'
import React from 'react'

import { ProductStatusBadge } from '@sourcegraph/wildcard'
import type { BaseProductStatusBadgeProps } from '@sourcegraph/wildcard/src/components/Badge'

interface FeedbackBadgeProps extends BaseProductStatusBadgeProps {
    /** Render a mailto href to share feedback */
    feedback: {
        mailto: string
        /** Defaults to 'Share feedback' */
        text?: string
    }
    className?: string
}

export const FeedbackBadge: React.FunctionComponent<FeedbackBadgeProps> = props => {
    const {
        className,
        status,
        tooltip,
        feedback: { mailto, text },
    } = props

    return (
        <div className={classNames('d-flex', 'align-items-center', className)}>
            <ProductStatusBadge tooltip={tooltip} status={status} className="text-uppercase" />
            <a href={`mailto:${mailto}`} className="ml-2" target="_blank" rel="noopener noreferrer">
                {text || 'Share feedback'}
            </a>
        </div>
    )
}
