import classNames from 'classnames'
import React, { HTMLAttributes } from 'react'

import styles from './TreeLayerCell.module.scss'

type TreeLayerCellProps = HTMLAttributes<HTMLTableCellElement>

export const TreeLayerCell: React.FunctionComponent<TreeLayerCellProps> = ({ className, children, ...rest }) => (
    <td className={classNames(className, styles.cell)} {...rest}>
        {children}
    </td>
)
