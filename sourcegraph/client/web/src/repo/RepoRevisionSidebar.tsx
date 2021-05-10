import { Tab, TabList, TabPanel, TabPanels, Tabs } from '@reach/tabs'
import classnames from 'classnames'
import * as H from 'history'
import ChevronDoubleLeftIcon from 'mdi-react/ChevronDoubleLeftIcon'
import FileTreeIcon from 'mdi-react/FileTreeIcon'
import React, { useCallback } from 'react'
import { Button } from 'reactstrap'

import { Resizable } from '@sourcegraph/shared/src/components/Resizable'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { AbsoluteRepoFile } from '@sourcegraph/shared/src/util/url'
import { useLocalStorage } from '@sourcegraph/shared/src/util/useLocalStorage'
import { useRedesignToggle } from '@sourcegraph/shared/src/util/useRedesignToggle'

import { Tree } from '../tree/Tree'

import { RepoRevisionSidebarSymbols } from './RepoRevisionSidebarSymbols'

interface Props extends AbsoluteRepoFile, ExtensionsControllerProps, ThemeProps {
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
    const [toggleSidebar, setToggleSidebar] = useLocalStorage(SIDEBAR_KEY, true)
    const [isRedesignEnabled] = useRedesignToggle()

    const handleTabsChange = useCallback((index: number) => setTabIndex(index), [setTabIndex])
    const handleSidebarToggle = useCallback(() => setToggleSidebar(!toggleSidebar), [setToggleSidebar, toggleSidebar])

    if (!toggleSidebar) {
        return (
            <button
                type="button"
                className="position-absolute btn btn-icon btn-link border-right border-bottom rounded-0 repo-revision-container__toggle"
                onClick={handleSidebarToggle}
                data-tooltip="Show sidebar"
            >
                <FileTreeIcon className="icon-inline" />
            </button>
        )
    }

    return (
        <Resizable
            defaultSize={256}
            handlePosition="right"
            storageKey={SIZE_STORAGE_KEY}
            element={
                <div className={classnames('d-flex w-100 border-right', !isRedesignEnabled && 'bg-2')}>
                    <Tabs
                        className="w-100 test-repo-revision-sidebar"
                        defaultIndex={tabIndex}
                        onChange={handleTabsChange}
                    >
                        <div className={classnames('tablist-wrapper d-flex flex-1', isRedesignEnabled && 'mx-3')}>
                            <TabList>
                                <Tab data-test-tab="files">
                                    <span className="tablist-wrapper--tab-label">Files</span>
                                </Tab>
                                <Tab data-test-tab="symbols">
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
                                <ChevronDoubleLeftIcon className="icon-inline repo-revision-container__close-icon" />
                            </Button>
                        </div>
                        <div
                            aria-hidden={true}
                            className={classnames(
                                'd-flex repo-revision-container__tabpanels explorer overflow-auto',
                                isRedesignEnabled && 'px-3'
                            )}
                        >
                            <TabPanels className="w-100">
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
                                            history={props.history}
                                            location={props.location}
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
