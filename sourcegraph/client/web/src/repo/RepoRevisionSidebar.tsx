import { Tab, TabList, TabPanel, TabPanels, Tabs } from '@reach/tabs'
import classNames from 'classnames'
import * as H from 'history'
import ChevronDoubleLeftIcon from 'mdi-react/ChevronDoubleLeftIcon'
import ChevronDoubleRightIcon from 'mdi-react/ChevronDoubleRightIcon'
import React, { useCallback } from 'react'
import { Button } from 'reactstrap'

import { Resizable } from '@sourcegraph/shared/src/components/Resizable'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { AbsoluteRepoFile } from '@sourcegraph/shared/src/util/url'
import { useLocalStorage } from '@sourcegraph/shared/src/util/useLocalStorage'

import settingsSchemaJSON from '../../../../schema/settings.schema.json'
import { Tree } from '../tree/Tree'

import styles from './RepoRevisionSidebar.module.scss'
import { RepoRevisionSidebarSymbols } from './RepoRevisionSidebarSymbols'

interface Props extends AbsoluteRepoFile, ExtensionsControllerProps, ThemeProps, TelemetryProps {
    repoID: Scalars['ID']
    isDir: boolean
    defaultBranch: string
    className: string
    history: H.History
    location: H.Location
}

const SIZE_STORAGE_KEY = 'repo-revision-sidebar'
const TABS_KEY = 'repo-revision-sidebar-last-tab'
const SIDEBAR_KEY = 'repo-revision-sidebar-toggle'
/**
 * The sidebar for a specific repo revision that shows the list of files and directories.
 */
export const RepoRevisionSidebar: React.FunctionComponent<Props> = props => {
    const [tabIndex, setTabIndex] = useLocalStorage(TABS_KEY, 0)
    const [toggleSidebar, setToggleSidebar] = useLocalStorage(
        SIDEBAR_KEY,
        settingsSchemaJSON.properties.sidebarToggle.default
    )

    const handleTabsChange = useCallback((index: number) => setTabIndex(index), [setTabIndex])
    const handleSidebarToggle = useCallback(() => {
        props.telemetryService.log('FileTreeViewClicked', {
            action: 'click',
            label: 'expand / collapse file tree view',
        })
        setToggleSidebar(!toggleSidebar)
    }, [setToggleSidebar, toggleSidebar, props.telemetryService])
    const handleSymbolClick = useCallback(() => props.telemetryService.log('SymbolTreeViewClicked'), [
        props.telemetryService,
    ])

    if (!toggleSidebar) {
        return (
            <button
                type="button"
                className={classNames(
                    'position-absolute btn btn-icon border-top border-bottom border-right mt-4',
                    styles.toggle
                )}
                onClick={handleSidebarToggle}
                data-tooltip="Show sidebar"
            >
                <ChevronDoubleRightIcon className="icon-inline" />
            </button>
        )
    }

    return (
        <Resizable
            defaultSize={256}
            handlePosition="right"
            storageKey={SIZE_STORAGE_KEY}
            element={
                <div className="d-flex w-100">
                    <Tabs
                        className="w-100 test-repo-revision-sidebar pr-3"
                        defaultIndex={tabIndex}
                        onChange={handleTabsChange}
                    >
                        <div className="tablist-wrapper d-flex flex-1">
                            <TabList>
                                <Tab data-tab-content="files">
                                    <span className="tablist-wrapper--tab-label">Files</span>
                                </Tab>
                                <Tab data-tab-content="symbols">
                                    <span className="tablist-wrapper--tab-label">Symbols</span>
                                </Tab>
                            </TabList>
                            <Button
                                onClick={handleSidebarToggle}
                                className="bg-transparent border-0 ml-auto p-1 position-relative focus-behaviour"
                                title="Close panel"
                                data-tooltip="Collapse panel"
                                data-placement="right"
                            >
                                <ChevronDoubleLeftIcon className={classNames('icon-inline', styles.closeIcon)} />
                            </Button>
                        </div>
                        <div aria-hidden={true} className={classNames('d-flex explorer', styles.tabpanels)}>
                            <TabPanels className="w-100 overflow-auto">
                                <TabPanel tabIndex={-1}>
                                    {tabIndex === 0 && (
                                        <Tree
                                            key="files"
                                            repoName={props.repoName}
                                            revision={props.revision}
                                            commitID={props.commitID}
                                            history={props.history}
                                            location={props.location}
                                            scrollRootSelector=".explorer"
                                            activePath={props.filePath}
                                            activePathIsDir={props.isDir}
                                            sizeKey={`Resizable:${SIZE_STORAGE_KEY}`}
                                            extensionsController={props.extensionsController}
                                            isLightTheme={props.isLightTheme}
                                            telemetryService={props.telemetryService}
                                        />
                                    )}
                                </TabPanel>
                                <TabPanel className="h-100">
                                    {tabIndex === 1 && (
                                        <RepoRevisionSidebarSymbols
                                            key="symbols"
                                            repoID={props.repoID}
                                            revision={props.revision}
                                            activePath={props.filePath}
                                            onHandleSymbolClick={handleSymbolClick}
                                        />
                                    )}
                                </TabPanel>
                            </TabPanels>
                        </div>
                    </Tabs>
                </div>
            }
        />
    )
}
