import * as H from 'history'
import React, { useState, useCallback, useMemo, useEffect, useContext } from 'react'
import { Subject } from 'rxjs'
import { repeatWhen, delay, withLatestFrom, map, filter, tap } from 'rxjs/operators'

import { createHoverifier } from '@sourcegraph/codeintellify'
import { ActionItemAction } from '@sourcegraph/shared/src/actions/ActionItem'
import { HoverMerged } from '@sourcegraph/shared/src/api/client/types/hover'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { getHoverActions } from '@sourcegraph/shared/src/hover/actions'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { property, isDefined } from '@sourcegraph/shared/src/util/types'
import { RepoSpec, RevisionSpec, FileSpec, ResolvedRevisionSpec } from '@sourcegraph/shared/src/util/url'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'
import { Container } from '@sourcegraph/wildcard'

import { getHover, getDocumentHighlights } from '../../../../backend/features'
import { FilteredConnection, FilteredConnectionQueryArguments } from '../../../../components/FilteredConnection'
import { WebHoverOverlay } from '../../../../components/shared'
import { AllChangesetIDsVariables, ChangesetFields, Scalars } from '../../../../graphql-operations'
import { MultiSelectContext, MultiSelectContextProvider } from '../../MultiSelectContext'
import { getLSPTextDocumentPositionParameters } from '../../utils'
import {
    queryChangesets as _queryChangesets,
    queryExternalChangesetWithFileDiffs as _queryExternalChangesetWithFileDiffs,
    queryAllChangesetIDs as _queryAllChangesetIDs,
} from '../backend'

import styles from './BatchChangeChangesets.module.scss'
import { BatchChangeChangesetsHeader, BatchChangeChangesetsHeaderProps } from './BatchChangeChangesetsHeader'
import { ChangesetFilters, ChangesetFilterRow } from './ChangesetFilterRow'
import { ChangesetNodeProps, ChangesetNode } from './ChangesetNode'
import { ChangesetSelectRow } from './ChangesetSelectRow'
import { EmptyArchivedChangesetListElement } from './EmptyArchivedChangesetListElement'
import { EmptyChangesetListElement } from './EmptyChangesetListElement'
import { EmptyChangesetSearchElement } from './EmptyChangesetSearchElement'

interface Props extends ThemeProps, PlatformContextProps, TelemetryProps, ExtensionsControllerProps {
    batchChangeID: Scalars['ID']
    viewerCanAdminister: boolean
    history: H.History
    location: H.Location

    hideFilters?: boolean
    onlyArchived?: boolean
    refetchBatchChange: () => void

    /** For testing only. */
    queryChangesets?: typeof _queryChangesets
    /** For testing only. */
    queryExternalChangesetWithFileDiffs?: typeof _queryExternalChangesetWithFileDiffs
    /** For testing only. */
    queryAllChangesetIDs?: typeof _queryAllChangesetIDs
    /** For testing only. */
    expandByDefault?: boolean
}

/**
 * A list of a batch change's changesets.
 */
export const BatchChangeChangesets: React.FunctionComponent<Props> = props => (
    <MultiSelectContextProvider>
        <BatchChangeChangesetsImpl {...props} />
    </MultiSelectContextProvider>
)

const BatchChangeChangesetsImpl: React.FunctionComponent<Props> = ({
    batchChangeID,
    viewerCanAdminister,
    history,
    location,
    isLightTheme,
    extensionsController,
    platformContext,
    telemetryService,
    hideFilters = false,
    queryChangesets = _queryChangesets,
    queryAllChangesetIDs = _queryAllChangesetIDs,
    queryExternalChangesetWithFileDiffs,
    expandByDefault,
    onlyArchived,
    refetchBatchChange,
}) => {
    // You might look at this destructuring statement and wonder why this isn't
    // just a single context consumer object. The reason is because making it a
    // single object makes it hard to have hooks that depend on individual
    // callbacks and objects within the context. Therefore, we'll have a nice,
    // ugly destructured set of variables here.
    const {
        selected,
        deselectAll,
        areAllVisibleSelected,
        isSelected,
        toggleSingle,
        toggleVisible,
        setVisible,
    } = useContext(MultiSelectContext)

    const [changesetFilters, setChangesetFilters] = useState<ChangesetFilters>({
        checkState: null,
        state: null,
        reviewState: null,
        search: null,
    })

    const setChangesetFiltersAndDeselectAll = useCallback(
        (filters: ChangesetFilters) => {
            deselectAll()
            setChangesetFilters(filters)
        },
        [deselectAll, setChangesetFilters]
    )

    // After selecting and performing a bulk action, deselect all changesets and refetch
    // the batch change to get the actively-running bulk operations.
    const onSubmitBulkAction = useCallback(() => {
        deselectAll()
        refetchBatchChange()
    }, [deselectAll, refetchBatchChange])

    const [queryArguments, setQueryArguments] = useState<Omit<AllChangesetIDsVariables, 'after'>>()

    const queryChangesetsConnection = useCallback(
        (args: FilteredConnectionQueryArguments) => {
            const passedArguments = {
                state: changesetFilters.state,
                reviewState: changesetFilters.reviewState,
                checkState: changesetFilters.checkState,
                first: args.first ?? null,
                after: args.after ?? null,
                batchChange: batchChangeID,
                onlyPublishedByThisBatchChange: null,
                search: changesetFilters.search,
                onlyArchived: !!onlyArchived,
            }
            return queryChangesets(passedArguments)
                .pipe(
                    tap(data => {
                        // Store the query arguments used for the current connection.
                        setQueryArguments(passedArguments)
                        // Available changesets are all changesets that the user
                        // can view.
                        setVisible(
                            data.nodes.filter(node => node.__typename === 'ExternalChangeset').map(node => node.id)
                        )
                    })
                )
                .pipe(repeatWhen(notifier => notifier.pipe(delay(5000))))
        },
        [
            changesetFilters.state,
            changesetFilters.reviewState,
            changesetFilters.checkState,
            changesetFilters.search,
            batchChangeID,
            onlyArchived,
            queryChangesets,
            setVisible,
        ]
    )

    const containerElements = useMemo(() => new Subject<HTMLElement | null>(), [])
    const nextContainerElement = useMemo(() => containerElements.next.bind(containerElements), [containerElements])

    const hoverOverlayElements = useMemo(() => new Subject<HTMLElement | null>(), [])
    const nextOverlayElement = useCallback((element: HTMLElement | null): void => hoverOverlayElements.next(element), [
        hoverOverlayElements,
    ])

    const closeButtonClicks = useMemo(() => new Subject<MouseEvent>(), [])
    const nextCloseButtonClick = useCallback((event: MouseEvent): void => closeButtonClicks.next(event), [
        closeButtonClicks,
    ])

    const componentRerenders = useMemo(() => new Subject<void>(), [])

    const hoverifier = useMemo(
        () =>
            createHoverifier<RepoSpec & RevisionSpec & FileSpec & ResolvedRevisionSpec, HoverMerged, ActionItemAction>({
                closeButtonClicks,
                hoverOverlayElements,
                hoverOverlayRerenders: componentRerenders.pipe(
                    withLatestFrom(hoverOverlayElements, containerElements),
                    map(([, hoverOverlayElement, relativeElement]) => ({
                        hoverOverlayElement,
                        // The root component element is guaranteed to be rendered after a componentDidUpdate
                        // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                        relativeElement: relativeElement!,
                    })),
                    // Can't reposition HoverOverlay if it wasn't rendered
                    filter(property('hoverOverlayElement', isDefined))
                ),
                getHover: hoveredToken =>
                    getHover(getLSPTextDocumentPositionParameters(hoveredToken), { extensionsController }),
                getDocumentHighlights: hoveredToken =>
                    getDocumentHighlights(getLSPTextDocumentPositionParameters(hoveredToken), { extensionsController }),
                getActions: context => getHoverActions({ extensionsController, platformContext }, context),
                pinningEnabled: true,
            }),
        [
            closeButtonClicks,
            containerElements,
            extensionsController,
            hoverOverlayElements,
            platformContext,
            componentRerenders,
        ]
    )
    useEffect(() => () => hoverifier.unsubscribe(), [hoverifier])

    const hoverState = useObservable(useMemo(() => hoverifier.hoverStateUpdates, [hoverifier]))
    useEffect(() => {
        componentRerenders.next()
    }, [componentRerenders, hoverState])

    const showSelectRow = viewerCanAdminister && (selected === 'all' || selected.size > 0)

    return (
        <Container>
            {!hideFilters && !showSelectRow && (
                <ChangesetFilterRow
                    history={history}
                    location={location}
                    onFiltersChange={setChangesetFiltersAndDeselectAll}
                />
            )}
            {showSelectRow && queryArguments && (
                <ChangesetSelectRow
                    batchChangeID={batchChangeID}
                    onSubmit={onSubmitBulkAction}
                    queryAllChangesetIDs={queryAllChangesetIDs}
                    queryArguments={queryArguments}
                />
            )}
            <div className="list-group position-relative" ref={nextContainerElement}>
                <FilteredConnection<ChangesetFields, Omit<ChangesetNodeProps, 'node'>, BatchChangeChangesetsHeaderProps>
                    nodeComponent={ChangesetNode}
                    nodeComponentProps={{
                        isLightTheme,
                        viewerCanAdminister,
                        history,
                        location,
                        extensionInfo: { extensionsController, hoverifier },
                        expandByDefault,
                        queryExternalChangesetWithFileDiffs,
                        selectable: { onSelect: toggleSingle, isSelected },
                    }}
                    queryConnection={queryChangesetsConnection}
                    hideSearch={true}
                    defaultFirst={15}
                    noun="changeset"
                    pluralNoun="changesets"
                    history={history}
                    location={location}
                    useURLQuery={true}
                    listComponent="div"
                    listClassName={styles.batchChangeChangesetsGrid}
                    withCenteredSummary={true}
                    headComponent={BatchChangeChangesetsHeader}
                    headComponentProps={{
                        allSelected: showSelectRow && areAllVisibleSelected(),
                        toggleSelectAll: toggleVisible,
                        disabled: !viewerCanAdminister,
                    }}
                    // Only show the empty element, if no filters are selected.
                    emptyElement={
                        filtersSelected(changesetFilters) ? (
                            <EmptyChangesetSearchElement />
                        ) : onlyArchived ? (
                            <EmptyArchivedChangesetListElement />
                        ) : (
                            <EmptyChangesetListElement />
                        )
                    }
                    noSummaryIfAllNodesVisible={true}
                />
                {hoverState?.hoverOverlayProps && (
                    <WebHoverOverlay
                        {...hoverState.hoverOverlayProps}
                        telemetryService={telemetryService}
                        extensionsController={extensionsController}
                        isLightTheme={isLightTheme}
                        location={location}
                        platformContext={platformContext}
                        hoverRef={nextOverlayElement}
                        onCloseButtonClick={nextCloseButtonClick}
                    />
                )}
            </div>
        </Container>
    )
}

/**
 * Returns true, if any filter is selected.
 */
function filtersSelected(filters: ChangesetFilters): boolean {
    return filters.checkState !== null || filters.state !== null || filters.reviewState !== null || !!filters.search
}
