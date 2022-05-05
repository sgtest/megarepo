import React, { useState } from 'react'

import classNames from 'classnames'
import { noop } from 'lodash'
import DotsVerticalIcon from 'mdi-react/DotsVerticalIcon'

import {
    Link,
    Menu,
    MenuButton,
    MenuDivider,
    MenuItem,
    MenuLink,
    MenuList,
    Position,
    Checkbox,
} from '@sourcegraph/wildcard'

import { useExperimentalFeatures } from '../../../../../../stores'
import { Insight, InsightDashboard, InsightType, isVirtualDashboard } from '../../../../core'
import { useUiFeatures } from '../../../../hooks/use-ui-features'
import { ConfirmDeleteModal } from '../../../modals/ConfirmDeleteModal'

import { ConfirmRemoveModal } from './ConfirmRemoveModal'

import styles from './InsightContextMenu.module.scss'

export interface InsightCardMenuProps {
    insight: Insight
    dashboard: InsightDashboard | null
    zeroYAxisMin: boolean
    menuButtonClassName?: string
    onToggleZeroYAxisMin?: () => void
}

/**
 * Renders context menu (three dots menu) for particular insight card.
 */
export const InsightContextMenu: React.FunctionComponent<React.PropsWithChildren<InsightCardMenuProps>> = props => {
    const { insight, dashboard, zeroYAxisMin, menuButtonClassName, onToggleZeroYAxisMin = noop } = props

    const [showDeleteConfirm, setShowDeleteConfirm] = useState(false)
    const [showRemoveConfirm, setShowRemoveConfirm] = useState(false)

    const { insight: insightPermissions } = useUiFeatures()
    const menuPermissions = insightPermissions.getContextActionsPermissions(insight)

    const insightID = insight.id
    const editUrl = dashboard?.id
        ? `/insights/edit/${insightID}?dashboardId=${dashboard.id}`
        : `/insights/edit/${insightID}`

    const features = useExperimentalFeatures()
    const showQuickFix = insight.title.includes('[quickfix]') && features?.goCodeCheckerTemplates

    const quickFixUrl =
        insight.type === InsightType.SearchBased
            ? `/batch-changes/create?kind=goChecker${insight.series[0]?.name}&title=${insight.title}`
            : undefined

    const withinVirtualDashboard = !!dashboard && isVirtualDashboard(dashboard)

    return (
        <>
            <Menu>
                {({ isOpen }) => (
                    <>
                        <MenuButton
                            data-testid="InsightContextMenuButton"
                            className={classNames(menuButtonClassName, 'p-1', styles.button)}
                            aria-label="Insight options"
                            outline={true}
                        >
                            <DotsVerticalIcon
                                className={classNames(styles.buttonIcon, { [styles.buttonIconActive]: isOpen })}
                                size={16}
                            />
                        </MenuButton>
                        <MenuList position={Position.bottomEnd} data-testid={`context-menu.${insightID}`}>
                            <MenuLink
                                as={Link}
                                data-testid="InsightContextMenuEditLink"
                                className={styles.item}
                                to={editUrl}
                            >
                                Edit
                            </MenuLink>

                            {menuPermissions.showYAxis && (
                                <MenuItem
                                    role="menuitemcheckbox"
                                    data-testid="InsightContextMenuEditLink"
                                    className={classNames('d-flex align-items-center justify-content-end', styles.item)}
                                    onSelect={onToggleZeroYAxisMin}
                                    aria-checked={zeroYAxisMin}
                                >
                                    <Checkbox
                                        aria-hidden="true"
                                        checked={zeroYAxisMin}
                                        onChange={noop}
                                        tabIndex={-1}
                                        id="InsightContextMenuEditInput"
                                        label={<span className="font-weight-normal">Start Y Axis at 0</span>}
                                    />
                                </MenuItem>
                            )}

                            {quickFixUrl && showQuickFix && (
                                <MenuLink as={Link} className={styles.item} to={quickFixUrl}>
                                    Golang quick fixes
                                </MenuLink>
                            )}

                            {dashboard && (
                                <MenuItem
                                    data-testid="insight-context-remove-from-dashboard-button"
                                    onSelect={() => setShowRemoveConfirm(true)}
                                    disabled={withinVirtualDashboard}
                                    data-tooltip={
                                        withinVirtualDashboard
                                            ? "Removing insight isn't available for the All insights dashboard"
                                            : undefined
                                    }
                                    data-placement="left"
                                    className={styles.item}
                                >
                                    Remove from this dashboard
                                </MenuItem>
                            )}

                            <MenuDivider />

                            <MenuItem
                                data-testid="insight-context-menu-delete-button"
                                onSelect={() => setShowDeleteConfirm(true)}
                                className={styles.item}
                            >
                                Delete
                            </MenuItem>
                        </MenuList>
                    </>
                )}
            </Menu>
            <ConfirmDeleteModal
                insight={insight}
                showModal={showDeleteConfirm}
                onCancel={() => setShowDeleteConfirm(false)}
            />
            <ConfirmRemoveModal
                insight={insight}
                dashboard={dashboard}
                showModal={showRemoveConfirm}
                onCancel={() => setShowRemoveConfirm(false)}
            />
        </>
    )
}
