import classNames from 'classnames'
import React from 'react'

import styles from './MarketingBlock.module.scss'

interface MarketingBlockProps {
    wrapperClassName?: string
    contentClassName?: string
}

export const MarketingBlock: React.FunctionComponent<MarketingBlockProps> = ({
    wrapperClassName,
    contentClassName,
    children,
}) => (
    <div className={classNames(styles.wrapper, wrapperClassName)}>
        <div className={classNames(styles.content, contentClassName)}>{children}</div>
    </div>
)
