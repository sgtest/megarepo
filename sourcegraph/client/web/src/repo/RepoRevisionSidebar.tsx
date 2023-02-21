import { FC, useCallback, useState } from 'react'

import { mdiChevronDoubleRight, mdiChevronDoubleLeft } from '@mdi/js'
import classNames from 'classnames'
import { useLocation, useNavigate } from 'react-router-dom'

import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { RepoFile } from '@sourcegraph/shared/src/util/url'
import {
    Button,
    useLocalStorage,
    useMatchMedia,
    Tab,
    TabList,
    TabPanel,
    TabPanels,
    Tabs,
    Icon,
    Panel,
    Tooltip,
} from '@sourcegraph/wildcard'

import settingsSchemaJSON from '../../../../schema/settings.schema.json'
import { AuthenticatedUser } from '../auth'
import { useFeatureFlag } from '../featureFlags/useFeatureFlag'
import { GettingStartedTour } from '../tour/GettingStartedTour'
import { Tree } from '../tree/Tree'

import { RepoRevisionSidebarFileTree } from './RepoRevisionSidebarFileTree'
import { RepoRevisionSidebarSymbols } from './RepoRevisionSidebarSymbols'

import styles from './RepoRevisionSidebar.module.scss'

interface RepoRevisionSidebarProps extends RepoFile, TelemetryProps, SettingsCascadeProps {
    repoID?: Scalars['ID']
    isDir: boolean
    defaultBranch: string
    className: string
    authenticatedUser: AuthenticatedUser | null
    isSourcegraphDotCom: boolean
}

const SIZE_STORAGE_KEY = 'repo-revision-sidebar'
const TABS_KEY = 'repo-revision-sidebar-last-tab'
const SIDEBAR_KEY = 'repo-revision-sidebar-toggle'
/**
 * The sidebar for a specific repo revision that shows the list of files and directories.
 */
export const RepoRevisionSidebar: FC<RepoRevisionSidebarProps> = props => {
    const location = useLocation()
    const navigate = useNavigate()

    const [persistedTabIndex, setPersistedTabIndex] = useLocalStorage(TABS_KEY, 0)
    const [persistedIsVisible, setPersistedIsVisible] = useLocalStorage(
        SIDEBAR_KEY,
        settingsSchemaJSON.properties.fileSidebarVisibleByDefault.default
    )
    const [enableAccessibleFileTree] = useFeatureFlag('accessible-file-tree')
    const [enableAccessibleFileTreeAlwaysLoadAncestors] = useFeatureFlag('accessible-file-tree-always-load-ancestors')

    const isWideScreen = useMatchMedia('(min-width: 768px)', false)
    const [isVisible, setIsVisible] = useState(persistedIsVisible && isWideScreen)

    const [initialFilePath, setInitialFilePath] = useState<string>(props.filePath)
    const [initialFilePathIsDir, setInitialFilePathIsDir] = useState<boolean>(props.isDir)
    const onExpandParent = useCallback((parent: string) => {
        setInitialFilePath(parent)
        setInitialFilePathIsDir(true)
    }, [])

    const handleSidebarToggle = useCallback(
        (value: boolean) => {
            props.telemetryService.log('FileTreeViewClicked', {
                action: 'click',
                label: 'expand / collapse file tree view',
            })
            setPersistedIsVisible(value)
            setIsVisible(value)
        },
        [setPersistedIsVisible, props.telemetryService]
    )
    const handleSymbolClick = useCallback(
        () => props.telemetryService.log('SymbolTreeViewClicked'),
        [props.telemetryService]
    )

    if (!isVisible) {
        return (
            <Tooltip content="Show sidebar">
                <Button
                    aria-label="Show sidebar"
                    variant="icon"
                    className={classNames(
                        'position-absolute border-top border-bottom border-right mt-4',
                        styles.toggle
                    )}
                    onClick={() => handleSidebarToggle(true)}
                >
                    <Icon aria-hidden={true} svgPath={mdiChevronDoubleRight} />
                </Button>
            </Tooltip>
        )
    }

    return (
        <Panel defaultSize={256} position="left" storageKey={SIZE_STORAGE_KEY} ariaLabel="File sidebar">
            <div className="d-flex flex-column h-100 w-100">
                <GettingStartedTour
                    className="mr-3"
                    telemetryService={props.telemetryService}
                    isAuthenticated={!!props.authenticatedUser}
                    isSourcegraphDotCom={props.isSourcegraphDotCom}
                />
                <Tabs
                    className="w-100 test-repo-revision-sidebar pr-3 h-25 d-flex flex-column flex-grow-1"
                    defaultIndex={persistedTabIndex}
                    onChange={setPersistedTabIndex}
                    lazy={true}
                >
                    <TabList
                        actions={
                            <Tooltip content="Hide sidebar" placement="right">
                                <Button
                                    aria-label="Hide sidebar"
                                    onClick={() => handleSidebarToggle(false)}
                                    className="bg-transparent border-0 ml-auto p-1 position-relative focus-behaviour"
                                >
                                    <Icon
                                        className={styles.closeIcon}
                                        aria-hidden={true}
                                        svgPath={mdiChevronDoubleLeft}
                                    />
                                </Button>
                            </Tooltip>
                        }
                    >
                        <Tab data-tab-content="files">
                            <span className="tablist-wrapper--tab-label">Files</span>
                        </Tab>
                        <Tab data-tab-content="symbols">
                            <span className="tablist-wrapper--tab-label">Symbols</span>
                        </Tab>
                    </TabList>
                    <div className={classNames('flex w-100 overflow-auto explorer', styles.tabpanels)} tabIndex={-1}>
                        {/* TODO: See if we can render more here, instead of waiting for these props */}
                        {props.repoID && props.commitID && (
                            <TabPanels>
                                <TabPanel>
                                    {enableAccessibleFileTree ? (
                                        <RepoRevisionSidebarFileTree
                                            key={initialFilePath}
                                            onExpandParent={onExpandParent}
                                            repoName={props.repoName}
                                            revision={props.revision}
                                            commitID={props.commitID}
                                            initialFilePath={initialFilePath}
                                            initialFilePathIsDirectory={initialFilePathIsDir}
                                            filePath={props.filePath}
                                            filePathIsDirectory={props.isDir}
                                            telemetryService={props.telemetryService}
                                            alwaysLoadAncestors={enableAccessibleFileTreeAlwaysLoadAncestors}
                                        />
                                    ) : (
                                        <Tree
                                            key="files"
                                            repoName={props.repoName}
                                            repoID={props.repoID}
                                            revision={props.revision}
                                            commitID={props.commitID}
                                            location={location}
                                            navigate={navigate}
                                            scrollRootSelector=".explorer"
                                            activePath={props.filePath}
                                            activePathIsDir={props.isDir}
                                            sizeKey={`Resizable:${SIZE_STORAGE_KEY}`}
                                            telemetryService={props.telemetryService}
                                        />
                                    )}
                                </TabPanel>
                                <TabPanel>
                                    <RepoRevisionSidebarSymbols
                                        key="symbols"
                                        repoID={props.repoID}
                                        revision={props.revision}
                                        activePath={props.filePath}
                                        onHandleSymbolClick={handleSymbolClick}
                                    />
                                </TabPanel>
                            </TabPanels>
                        )}
                    </div>
                </Tabs>
            </div>
        </Panel>
    )
}
