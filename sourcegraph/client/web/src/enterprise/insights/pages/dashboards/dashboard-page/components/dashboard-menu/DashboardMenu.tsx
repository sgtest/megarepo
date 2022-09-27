import React from 'react'

import { mdiDotsVertical } from '@mdi/js'
import classNames from 'classnames'

import {
    Button,
    Icon,
    Menu,
    MenuButton,
    MenuDivider,
    MenuItem,
    MenuList,
    Position,
    Tooltip,
} from '@sourcegraph/wildcard'

import { InsightDashboard } from '../../../../../core'
import { useUiFeatures } from '../../../../../hooks'

import styles from './DashboardMenu.module.scss'

export enum DashboardMenuAction {
    CopyLink,
    Delete,
    Configure,
    AddRemoveInsights,
}

export interface DashboardMenuProps {
    innerRef: React.Ref<HTMLButtonElement>
    dashboard?: InsightDashboard
    onSelect?: (action: DashboardMenuAction) => void
    tooltipText?: string
    className?: string
}

export const DashboardMenu: React.FunctionComponent<React.PropsWithChildren<DashboardMenuProps>> = props => {
    const { innerRef, dashboard, onSelect = () => {}, tooltipText, className } = props

    const { dashboard: dashboardPermission } = useUiFeatures()
    const menuPermissions = dashboardPermission.getContextActionsPermissions(dashboard)

    return (
        <Menu>
            <Tooltip content={tooltipText} placement="right">
                <MenuButton
                    ref={innerRef}
                    variant="icon"
                    outline={true}
                    className={classNames(className, styles.triggerButton)}
                >
                    <Icon
                        svgPath={mdiDotsVertical}
                        inline={false}
                        height={16}
                        width={16}
                        aria-label="dashboard options"
                    />
                </MenuButton>
            </Tooltip>

            <MenuList className={styles.menuList} position={Position.bottomEnd}>
                {menuPermissions.configure.display && (
                    <Tooltip content={menuPermissions.configure.tooltip} placement="right">
                        <MenuItem
                            as={Button}
                            outline={true}
                            disabled={menuPermissions.configure.disabled}
                            className={styles.menuItem}
                            onSelect={() => onSelect(DashboardMenuAction.Configure)}
                        >
                            Configure dashboard
                        </MenuItem>
                    </Tooltip>
                )}

                {menuPermissions.copy.display && (
                    <MenuItem
                        as={Button}
                        outline={true}
                        disabled={menuPermissions.copy.disabled}
                        className={styles.menuItem}
                        data-testid="copy-link"
                        onSelect={() => onSelect(DashboardMenuAction.CopyLink)}
                    >
                        Copy link
                    </MenuItem>
                )}

                {(menuPermissions.configure.display || menuPermissions.copy.display) &&
                    menuPermissions.delete.display && <MenuDivider />}

                {menuPermissions.delete.display && (
                    <Tooltip content={menuPermissions.delete.tooltip} placement="right">
                        <MenuItem
                            as={Button}
                            outline={true}
                            disabled={menuPermissions.delete.disabled}
                            className={classNames(styles.menuItem, styles.menuItemDanger)}
                            onSelect={() => onSelect(DashboardMenuAction.Delete)}
                        >
                            Delete
                        </MenuItem>
                    </Tooltip>
                )}
            </MenuList>
        </Menu>
    )
}
