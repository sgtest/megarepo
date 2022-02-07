import classNames from 'classnames'
import MenuUpIcon from 'mdi-react/MenuUpIcon'
import React, { useCallback } from 'react'
import { UncontrolledPopover } from 'reactstrap'

import { Button, Card, Tab, TabList, TabPanel, TabPanels, Tabs, useLocalStorage } from '@sourcegraph/wildcard'

import { PlatformContextProps } from '../../platform/context'
import { ExtensionsControllerProps } from '../controller'

import { ActiveExtensionsPanel } from './ActiveExtensionsPanel'
import styles from './index.module.scss'

export interface ExtensionsDevelopmentToolsProps
    extends ExtensionsControllerProps,
        PlatformContextProps<'sideloadedExtensionURL' | 'settings'> {
    link: React.ComponentType<{ id: string }>
}

const LAST_TAB_STORAGE_KEY = 'ExtensionDevTools.lastTab'

type ExtensionDevelopmentToolsTabID = 'activeExtensions' | 'loggers'

interface ExtensionDevelopmentToolsTab {
    id: ExtensionDevelopmentToolsTabID
    label: string
    component: React.ComponentType<ExtensionsDevelopmentToolsProps>
}

const TABS: ExtensionDevelopmentToolsTab[] = [
    { id: 'activeExtensions', label: 'Active extensions', component: ActiveExtensionsPanel },
]

const ExtensionDevelopmentTools: React.FunctionComponent<ExtensionsDevelopmentToolsProps> = props => {
    const [tabIndex, setTabIndex] = useLocalStorage(LAST_TAB_STORAGE_KEY, 0)
    const handleTabsChange = useCallback((index: number) => setTabIndex(index), [setTabIndex])

    return (
        <Tabs
            as={Card}
            defaultIndex={tabIndex}
            className={classNames('border-0 rounded-0', styles.extensionStatus)}
            onChange={handleTabsChange}
        >
            <TabList>
                {TABS.map(({ label, id }) => (
                    <Tab key={id} data-tab-content={id}>
                        {label}
                    </Tab>
                ))}
            </TabList>

            <TabPanels>
                {TABS.map(tab => (
                    <TabPanel key={tab.id}>
                        <tab.component {...props} />
                    </TabPanel>
                ))}
            </TabPanels>
        </Tabs>
    )
}

/** A button that toggles the visibility of the ExtensionDevTools element in a popover. */
export const ExtensionDevelopmentToolsPopover = React.memo<ExtensionsDevelopmentToolsProps>(props => (
    <>
        <Button id="extension-status-popover" className="text-decoration-none px-2" variant="link">
            <span className="text-muted">Ext</span> <MenuUpIcon className="icon-inline" />
        </Button>
        <UncontrolledPopover
            placement="auto-end"
            target="extension-status-popover"
            hideArrow={true}
            popperClassName="border-0 rounded-0"
        >
            <ExtensionDevelopmentTools {...props} />
        </UncontrolledPopover>
    </>
))
