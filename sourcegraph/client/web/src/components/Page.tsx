import classNames from 'classnames'
import React from 'react'

export const Page: React.FunctionComponent<{ className?: string }> = ({ className, children }) => (
    <div className={classNames('container web-content py-4', className)}>{children}</div>
)
