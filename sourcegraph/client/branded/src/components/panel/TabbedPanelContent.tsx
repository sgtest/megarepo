import React, { useCallback, useEffect, useMemo, useState } from 'react'

import { mdiClose } from '@mdi/js'
import classNames from 'classnames'
import { useLocation, useNavigate } from 'react-router-dom'
import { BehaviorSubject, type Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import type { FetchFileParameters } from '@sourcegraph/shared/src/backend/file'
import type { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import type { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import type { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    Button,
    useObservable,
    Tab,
    TabList,
    TabPanel,
    TabPanels,
    Tabs,
    Icon,
    Tooltip,
    useKeyboard,
    type ProductStatusType,
    ProductStatusBadge,
} from '@sourcegraph/wildcard'

import { MixPreciseAndSearchBasedReferencesToggle } from './MixPreciseAndSearchBasedReferencesToggle'
import { EmptyPanelView } from './views/EmptyPanelView'

import styles from './TabbedPanelContent.module.scss'

interface TabbedPanelContentProps extends PlatformContextProps, SettingsCascadeProps, TelemetryProps {
    repoName?: string
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

/**
 * A tab and corresponding content to display in the panel.
 */
export interface Panel {
    /** The ID of the panel. */
    id: string

    /** The title of the panel view. */
    title: string

    /** Optional product status to show as a badge next to the panel title. */
    productStatus?: ProductStatusType

    /** The content element to display when the tab is active. */
    element: React.ReactNode

    // Should the panel be shown for the given `#tab=<ID>` in the URL?
    matchesTabID?: (id: string) => boolean

    /** Callback that's triggered when the panel is selected */
    trackTabClick?: () => void
}

const panelRegistry = new BehaviorSubject<Panel[]>([])

/**
 * React hook for other components to add panels.
 */
export function useBuiltinTabbedPanelViews(panels: Panel[]): void {
    useEffect(() => {
        panelRegistry.next([
            ...panelRegistry.value.filter(panel => !panels.some(({ id }) => panel.id === id)),
            ...panels,
        ])

        return () => {
            panelRegistry.next(panelRegistry.value.filter(panel => !panels.some(({ id }) => panel.id === id)))
        }
    }, [panels])
}

/**
 * The panel, which is a tabbed component with contextual information. Components rendering the panel should
 * generally use ResizablePanel, not Panel.
 *
 * Other components can contribute panel items to the panel with the `useBuildinPanelViews` hook.
 */
export const TabbedPanelContent = React.memo<TabbedPanelContentProps>(props => {
    const [tabIndex, setTabIndex] = useState(0)
    const { hash, pathname, search } = useLocation()
    const navigate = useNavigate()

    const handlePanelClose = useCallback(() => navigate(pathname, { replace: true }), [navigate, pathname])
    const [currentTabLabel, currentTabID] = hash.split('=')

    const trackTabClick = useCallback(
        (title: string) => props.telemetryService.log(`ReferencePanelClicked${title}`),
        [props.telemetryService]
    )

    const panels: Panel[] | undefined = useObservable(
        useMemo(
            () =>
                panelRegistry.pipe(
                    map(panels =>
                        panels.map(panel => ({
                            ...panel,
                            trackTabClick: () => trackTabClick(panel.title),
                        }))
                    )
                ),
            [trackTabClick]
        )
    )

    useKeyboard({ detectKeys: ['Escape'] }, handlePanelClose)

    const handleActiveTab = useCallback(
        (index: number): void => {
            navigate(`${pathname}${search}${currentTabLabel}=${panels ? panels[index].id : ''}`, { replace: true })
        },
        [currentTabLabel, navigate, panels, pathname, search]
    )

    useEffect(() => {
        setTabIndex(
            panels
                ? panels.findIndex(({ id, matchesTabID }) =>
                      matchesTabID ? matchesTabID(currentTabID) : id === currentTabID
                  )
                : 0
        )
    }, [panels, hash, currentTabID])

    if (!panels) {
        return <EmptyPanelView className={styles.panel} />
    }

    const activeTab: Panel | undefined = panels[tabIndex]

    return (
        <Tabs className={styles.panel} index={tabIndex} onChange={handleActiveTab}>
            <TabList
                wrapperClassName={classNames(styles.panelHeader, 'sticky-top')}
                actions={
                    <div className="align-items-center d-flex">
                        <ul className="d-flex justify-content-end list-unstyled m-0 align-items-center">
                            {activeTab && activeTab.id === 'references' && (
                                <MixPreciseAndSearchBasedReferencesToggle
                                    settingsCascade={props.settingsCascade}
                                    platformContext={props.platformContext}
                                />
                            )}
                        </ul>
                        <Tooltip content="Close panel" placement="left">
                            <Button
                                onClick={handlePanelClose}
                                variant="icon"
                                className={classNames('ml-2', styles.dismissButton)}
                                title="Close panel"
                            >
                                <Icon aria-hidden={true} svgPath={mdiClose} />
                            </Button>
                        </Tooltip>
                    </div>
                }
            >
                {panels.map(({ title, id, trackTabClick, productStatus }, index) => (
                    <Tab key={id} index={index}>
                        <span className="tablist-wrapper--tab-label" onClick={trackTabClick} role="none">
                            {title}
                            {productStatus && (
                                <>
                                    {' '}
                                    <ProductStatusBadge status={productStatus} />
                                </>
                            )}
                        </span>
                    </Tab>
                ))}
            </TabList>
            <TabPanels>
                {activeTab ? (
                    panels.map(({ id, element }, index) => (
                        <TabPanel
                            index={index}
                            key={id}
                            className={styles.tabsContent}
                            data-testid="panel-tabs-content"
                        >
                            {id === activeTab.id ? element : null}
                        </TabPanel>
                    ))
                ) : (
                    <EmptyPanelView />
                )}
            </TabPanels>
        </Tabs>
    )
})

TabbedPanelContent.displayName = 'TabbedPanelContent'
