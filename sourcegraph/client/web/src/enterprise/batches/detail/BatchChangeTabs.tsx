import React, { useState, useCallback, useEffect } from 'react'
import * as H from 'history'
import { ExtensionsControllerProps } from '../../../../../shared/src/extensions/controller'
import { ThemeProps } from '../../../../../shared/src/theme'
import { PlatformContextProps } from '../../../../../shared/src/platform/context'
import { TelemetryProps } from '../../../../../shared/src/telemetry/telemetryService'
import { BatchChangeFields } from '../../../graphql-operations'
import {
    queryChangesets as _queryChangesets,
    queryExternalChangesetWithFileDiffs as _queryExternalChangesetWithFileDiffs,
    queryChangesetCountsOverTime as _queryChangesetCountsOverTime,
} from './backend'
import classNames from 'classnames'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'
import ChartLineVariantIcon from 'mdi-react/ChartLineVariantIcon'
import { BatchChangeBurndownChart } from './BatchChangeBurndownChart'
import { BatchChangeChangesets } from './changesets/BatchChangeChangesets'
import FileDocumentIcon from 'mdi-react/FileDocumentIcon'
import ArchiveIcon from 'mdi-react/ArchiveIcon'
import { BatchSpecTab } from './BatchSpecTab'

type SelectedTab = 'changesets' | 'chart' | 'spec' | 'archived'

export interface BatchChangeTabsProps
    extends ExtensionsControllerProps,
        ThemeProps,
        PlatformContextProps,
        TelemetryProps {
    batchChange: BatchChangeFields
    changesetsCount: number
    archivedCount: number
    history: H.History
    location: H.Location
    /** For testing only. */
    queryChangesets?: typeof _queryChangesets
    /** For testing only. */
    queryExternalChangesetWithFileDiffs?: typeof _queryExternalChangesetWithFileDiffs
    /** For testing only. */
    queryChangesetCountsOverTime?: typeof _queryChangesetCountsOverTime
}

export const BatchChangeTabs: React.FunctionComponent<BatchChangeTabsProps> = ({
    extensionsController,
    history,
    isLightTheme,
    location,
    platformContext,
    telemetryService,
    batchChange,
    changesetsCount,
    archivedCount,
    queryChangesets,
    queryChangesetCountsOverTime,
    queryExternalChangesetWithFileDiffs,
}) => {
    const archiveEnabled = window.context?.experimentalFeatures?.archiveBatchChangeChangesets ?? false
    const [selectedTab, setSelectedTab] = useState<SelectedTab>(
        selectedTabFromLocation(location.search, archiveEnabled)
    )
    useEffect(() => {
        const newTab = selectedTabFromLocation(location.search, archiveEnabled)
        if (newTab !== selectedTab) {
            setSelectedTab(newTab)
        }
    }, [location.search, selectedTab, archiveEnabled])

    const onSelectChangesets = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setSelectedTab('changesets')
            const urlParameters = new URLSearchParams(location.search)
            urlParameters.delete('tab')
            if (location.search !== urlParameters.toString()) {
                history.replace({ ...location, search: urlParameters.toString() })
            }
        },
        [history, location]
    )
    const onSelectChart = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setSelectedTab('chart')
            const urlParameters = new URLSearchParams(location.search)
            urlParameters.set('tab', 'chart')
            if (location.search !== urlParameters.toString()) {
                history.replace({ ...location, search: urlParameters.toString() })
            }
        },
        [history, location]
    )
    const onSelectSpec = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setSelectedTab('spec')
            const urlParameters = new URLSearchParams(location.search)
            urlParameters.set('tab', 'spec')
            if (location.search !== urlParameters.toString()) {
                history.replace({ ...location, search: urlParameters.toString() })
            }
        },
        [history, location]
    )
    const onSelectArchived = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setSelectedTab('archived')
            const urlParameters = new URLSearchParams(location.search)
            urlParameters.set('tab', 'archived')
            if (location.search !== urlParameters.toString()) {
                history.replace({ ...location, search: urlParameters.toString() })
            }
        },
        [history, location]
    )

    return (
        <>
            <div className="overflow-auto mb-2">
                <ul className="nav nav-tabs d-inline-flex d-sm-flex flex-nowrap text-nowrap">
                    <li className="nav-item">
                        {/* eslint-disable-next-line jsx-a11y/anchor-is-valid */}
                        <a
                            href=""
                            role="button"
                            onClick={onSelectChangesets}
                            className={classNames('nav-link', selectedTab === 'changesets' && 'active')}
                        >
                            <SourceBranchIcon className="icon-inline text-muted mr-1" />
                            Changesets <span className="badge badge-pill badge-secondary ml-1">{changesetsCount}</span>
                        </a>
                    </li>
                    <li className="nav-item test-batches-chart-tab">
                        {/* eslint-disable-next-line jsx-a11y/anchor-is-valid */}
                        <a
                            href=""
                            role="button"
                            onClick={onSelectChart}
                            className={classNames('nav-link', selectedTab === 'chart' && 'active')}
                        >
                            <ChartLineVariantIcon className="icon-inline text-muted mr-1" /> Burndown chart
                        </a>
                    </li>
                    <li className="nav-item test-batches-spec-tab">
                        {/* eslint-disable-next-line jsx-a11y/anchor-is-valid */}
                        <a
                            href=""
                            role="button"
                            onClick={onSelectSpec}
                            className={classNames('nav-link', selectedTab === 'spec' && 'active')}
                        >
                            <FileDocumentIcon className="icon-inline text-muted mr-1" /> Spec
                        </a>
                    </li>
                    {archiveEnabled && (
                        <li className="nav-item">
                            {/* eslint-disable-next-line jsx-a11y/anchor-is-valid */}
                            <a
                                href=""
                                role="button"
                                onClick={onSelectArchived}
                                className={classNames('nav-link', selectedTab === 'archived' && 'active')}
                            >
                                <ArchiveIcon className="icon-inline text-muted mr-1" /> Archived{' '}
                                <span className="badge badge-pill badge-secondary ml-1">{archivedCount}</span>
                            </a>
                        </li>
                    )}
                </ul>
            </div>
            {selectedTab === 'chart' && (
                <BatchChangeBurndownChart
                    batchChangeID={batchChange.id}
                    queryChangesetCountsOverTime={queryChangesetCountsOverTime}
                    history={history}
                />
            )}
            {selectedTab === 'changesets' && (
                <BatchChangeChangesets
                    batchChangeID={batchChange.id}
                    viewerCanAdminister={batchChange.viewerCanAdminister}
                    history={history}
                    location={location}
                    isLightTheme={isLightTheme}
                    extensionsController={extensionsController}
                    platformContext={platformContext}
                    telemetryService={telemetryService}
                    queryChangesets={queryChangesets}
                    queryExternalChangesetWithFileDiffs={queryExternalChangesetWithFileDiffs}
                    onlyArchived={false}
                />
            )}
            {selectedTab === 'spec' && (
                <BatchSpecTab batchChange={batchChange} originalInput={batchChange.currentSpec.originalInput} />
            )}
            {selectedTab === 'archived' && (
                <BatchChangeChangesets
                    batchChangeID={batchChange.id}
                    viewerCanAdminister={batchChange.viewerCanAdminister}
                    history={history}
                    location={location}
                    isLightTheme={isLightTheme}
                    extensionsController={extensionsController}
                    platformContext={platformContext}
                    telemetryService={telemetryService}
                    queryChangesets={queryChangesets}
                    queryExternalChangesetWithFileDiffs={queryExternalChangesetWithFileDiffs}
                    onlyArchived={true}
                />
            )}
        </>
    )
}

function selectedTabFromLocation(locationSearch: string, archiveEnabled: boolean): SelectedTab {
    const urlParameters = new URLSearchParams(locationSearch)
    if (urlParameters.get('tab') === 'chart') {
        return 'chart'
    }
    if (urlParameters.get('tab') === 'spec') {
        return 'spec'
    }
    if (urlParameters.get('tab') === 'archived') {
        return archiveEnabled ? 'archived' : 'changesets'
    }
    return 'changesets'
}
