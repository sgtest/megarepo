import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef } from 'react'

import classNames from 'classnames'
import { Remote } from 'comlink'
import * as H from 'history'
import { isEqual } from 'lodash'
import {
    BehaviorSubject,
    combineLatest,
    merge,
    EMPTY,
    from,
    fromEvent,
    ReplaySubject,
    Subscription,
    Subject,
} from 'rxjs'
import { concatMap, filter, first, map, mapTo, pairwise, switchMap, tap, withLatestFrom } from 'rxjs/operators'
import useDeepCompareEffect from 'use-deep-compare-effect'

import { HoverMerged } from '@sourcegraph/client-api'
import {
    getCodeElementsInRange,
    HoveredToken,
    locateTarget,
    findPositionsFromEvents,
    createHoverifier,
    HoverState,
} from '@sourcegraph/codeintellify'
import {
    isErrorLike,
    isDefined,
    property,
    LineOrPositionOrRange,
    lprToSelectionsZeroIndexed,
    toPositionOrRangeQueryParameter,
    addLineRangeQueryParameter,
    formatSearchParameters,
    logger,
} from '@sourcegraph/common'
import { ActionItemAction } from '@sourcegraph/shared/src/actions/ActionItem'
import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { haveInitialExtensionsLoaded } from '@sourcegraph/shared/src/api/features'
import { ViewerId } from '@sourcegraph/shared/src/api/viewerTypes'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { getHoverActions } from '@sourcegraph/shared/src/hover/actions'
import { HoverContext, PinOptions } from '@sourcegraph/shared/src/hover/HoverOverlay'
import { getModeFromPath } from '@sourcegraph/shared/src/languages'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { Settings, SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { codeCopiedEvent } from '@sourcegraph/shared/src/tracking/event-log-creators'
import {
    AbsoluteRepoFile,
    FileSpec,
    ModeSpec,
    UIPositionSpec,
    RepoSpec,
    ResolvedRevisionSpec,
    RevisionSpec,
    toURIWithPath,
    parseQueryAndHash,
} from '@sourcegraph/shared/src/util/url'
import { Code, useObservable } from '@sourcegraph/wildcard'

import { getHover, getDocumentHighlights } from '../../backend/features'
import { WebHoverOverlay } from '../../components/shared'
import { BlobStencilFields, ExternalLinkFields, Scalars } from '../../graphql-operations'
import { BlameHunkData } from '../blame/useBlameHunks'
import { HoverThresholdProps } from '../RepoContainer'

import { BlameColumn } from './BlameColumn'

import styles from './Blob.module.scss'

// Logical grouping of props that are only used by the CodeMirror blob view
// implementation.
interface CodeMirrorBlobProps {
    overrideBrowserSearchKeybinding?: boolean
}

export interface BlobProps
    extends SettingsCascadeProps,
        PlatformContextProps,
        TelemetryProps,
        HoverThresholdProps,
        ExtensionsControllerProps,
        ThemeProps,
        CodeMirrorBlobProps {
    location: H.Location
    history: H.History
    className: string
    wrapCode: boolean
    /** The current text document to be rendered and provided to extensions */
    blobInfo: BlobInfo
    'data-testid'?: string

    // When navigateToLineOnAnyClick=true, the code intel popover is disabled
    // and clicking on any line should navigate to that specific line.
    navigateToLineOnAnyClick?: boolean

    // Enables experimental navigation by rendering links for all interactive tokens.
    enableLinkDrivenCodeNavigation?: boolean
    // Enables experimental navigation by making interactive tokens selectable on click.
    enableSelectionDrivenCodeNavigation?: boolean

    // If set, nav is called when a user clicks on a token highlighted by
    // WebHoverOverlay
    nav?: (url: string) => void
    role?: string
    ariaLabel?: string

    supportsFindImplementations?: boolean

    isBlameVisible?: boolean
    blameHunks?: BlameHunkData
}

export interface BlobInfo extends AbsoluteRepoFile, ModeSpec {
    /** The raw content of the blob. */
    content: string

    /** The trusted syntax-highlighted code as HTML */
    html: string

    /** LSIF syntax-highlighting data */
    lsif?: string

    stencil?: BlobStencilFields[]

    /** If present, the file is stored in Git LFS (large file storage). */
    lfs?: { byteSize: Scalars['BigInt'] } | null

    /** External URLs for the file */
    externalURLs?: ExternalLinkFields[]
}

const domFunctions = {
    getCodeElementFromTarget: (target: HTMLElement): HTMLTableCellElement | null => {
        // If the target is part of the line decoration attachment, return null.
        if (
            target.hasAttribute('data-line-decoration-attachment') ||
            target.hasAttribute('data-line-decoration-attachment-content')
        ) {
            return null
        }

        const row = target.closest('tr')
        if (!row) {
            return null
        }
        return row.querySelector('td.code')
    },
    getCodeElementFromLineNumber: (codeView: HTMLElement, line: number): HTMLTableCellElement | null => {
        const table = codeView.firstElementChild as HTMLTableElement
        const row = table.rows[line - 1]
        if (!row) {
            return null
        }
        return row.querySelector('td.code')
    },
    getLineNumberFromCodeElement: (codeCell: HTMLElement): number => {
        const row = codeCell.closest('tr')
        if (!row) {
            throw new Error('Could not find closest row for codeCell')
        }
        const numberCell = row.querySelector<HTMLTableCellElement>('td.line')
        if (!numberCell || !numberCell.dataset.line) {
            throw new Error('Could not find line number')
        }
        return parseInt(numberCell.dataset.line, 10)
    },
}

/**
 * Renders a code view augmented by Sourcegraph extensions
 *
 * Documentation:
 *
 * What is the difference between blobInfoChanges and viewerUpdates?
 *
 * - blobInfoChanges: emits when document info has loaded from the backend (including raw HTML)
 * - viewerUpdates: emits when the extension host confirms that it knows about the current viewer.
 * message to extension host is sent on each blobInfo change, and when that message receives a response
 * with the viewerId (handle to viewer on extension host side), viewerUpdates emits it along with
 * other data (such as subscriptions, extension host API) relevant to observers for this viewer.
 *
 * The possible states that Blob can be in:
 * - "extension host bootstrapping": Initial page load, the initial set of extensions
 * haven't been loaded yet. Regardless of whether or not the extension host knows about
 * the current viewer, users can't interact with extensions yet.
 * - "extension host ready": Extensions have loaded, extension host knows about the current viewer
 * - "extension host loading viewer": Extensions have loaded, but the extension host
 * doesn't know about the current viewer yet. We know that we are in this state
 * when blobInfo changes. On entering this state, clear resources from
 * previous viewer (e.g. hoverifier subscription). If we don't remove extension features
 * in this state, hovers can lead to errors like `DocumentNotFoundError`.
 */
export const Blob: React.FunctionComponent<React.PropsWithChildren<BlobProps>> = props => {
    const {
        location,
        isLightTheme,
        extensionsController,
        blobInfo,
        platformContext,
        settingsCascade,
        role,
        ariaLabel,
        'data-testid': dataTestId,
    } = props

    const settingsChanges = useMemo(() => new BehaviorSubject<Settings | null>(null), [])
    useEffect(() => {
        if (
            settingsCascade.final &&
            !isErrorLike(settingsCascade.final) &&
            (!settingsChanges.value || !isEqual(settingsChanges.value, settingsCascade.final))
        ) {
            settingsChanges.next(settingsCascade.final)
        }
    }, [settingsCascade, settingsChanges])

    // Element reference subjects passed to `hoverifier`
    const blobElements = useMemo(() => new ReplaySubject<HTMLElement | null>(1), [])
    const nextBlobElement = useCallback(
        (blobElement: HTMLElement | null) => blobElements.next(blobElement),
        [blobElements]
    )

    const hoverOverlayElements = useMemo(() => new ReplaySubject<HTMLElement | null>(1), [])
    const nextOverlayElement = useCallback(
        (overlayElement: HTMLElement | null) => hoverOverlayElements.next(overlayElement),
        [hoverOverlayElements]
    )

    const codeViewElements = useMemo(() => new ReplaySubject<HTMLElement | null>(1), [])
    const codeViewReference = useRef<HTMLElement | null>()
    const nextCodeViewElement = useCallback(
        (codeView: HTMLElement | null) => {
            codeViewReference.current = codeView
            codeViewElements.next(codeView)
        },
        // We dangerousSetInnerHTML and modify the <code> element.
        // We need to listen to blobInfo to ensure that we correctly
        // respond whenever this element updates.
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [codeViewElements, blobInfo.html]
    )

    // Emits on changes from URL search params
    const urlSearchParameters = useMemo(() => new ReplaySubject<URLSearchParams>(1), [])
    const nextUrlSearchParameters = useCallback(
        (value: URLSearchParams) => urlSearchParameters.next(value),
        [urlSearchParameters]
    )
    useEffect(() => {
        nextUrlSearchParameters(new URLSearchParams(location.search))
    }, [nextUrlSearchParameters, location.search])

    // Emits on position changes from URL hash
    const locationPositions = useMemo(() => new ReplaySubject<LineOrPositionOrRange>(1), [])
    const nextLocationPosition = useCallback(
        (lineOrPositionOrRange: LineOrPositionOrRange) => locationPositions.next(lineOrPositionOrRange),
        [locationPositions]
    )
    const parsedHash = useMemo(
        () => parseQueryAndHash(location.search, location.hash),
        [location.search, location.hash]
    )
    useDeepCompareEffect(() => {
        nextLocationPosition(parsedHash)
    }, [parsedHash])

    // Subject that emits on every render. Source for `hoverOverlayRerenders`, used to
    // reposition hover overlay if needed when `Blob` rerenders
    const rerenders = useMemo(() => new ReplaySubject(1), [])
    useEffect(() => {
        rerenders.next()
    })

    // Emits on blob info changes to update extension host model
    const blobInfoChanges = useMemo(() => new ReplaySubject<BlobInfo>(1), [])
    const nextBlobInfoChange = useCallback((blobInfo: BlobInfo) => blobInfoChanges.next(blobInfo), [blobInfoChanges])

    const viewerUpdates = useMemo(
        () =>
            new BehaviorSubject<{
                viewerId: ViewerId
                blobInfo: BlobInfo
                extensionHostAPI: Remote<FlatExtensionHostAPI>
                subscriptions: Subscription
            } | null>(null),
        []
    )

    // Need to use deep compare effect to avoid infinite loop in the tabbed references panel.
    useDeepCompareEffect(() => {
        nextBlobInfoChange(blobInfo)
        return () => {
            // Clean up for any resources used by the previous viewer.
            // We can't wait for + don't care about the round trip of
            // client (blobInfo change) -> ext host (add viewer) -> client (receive viewerId)
            // that viewerUpdates emits after.
            viewerUpdates.value?.subscriptions.unsubscribe()

            // Clear viewerUpdates to signify that we are in the "extension host loading viewer" state
            viewerUpdates.next(null)
        }
    }, [blobInfo, nextBlobInfoChange, viewerUpdates])

    const popoverCloses = useMemo(() => new Subject<void>(), [])
    const nextPopoverClose = useCallback((click: void) => popoverCloses.next(click), [popoverCloses])

    useObservable(
        useMemo(
            () =>
                popoverCloses.pipe(
                    withLatestFrom(urlSearchParameters),
                    tap(([, parameters]) => {
                        parameters.delete('popover')
                        updateBrowserHistoryIfChanged(props.history, location, parameters)
                    })
                ),
            [location, popoverCloses, props.history, urlSearchParameters]
        )
    )

    const popoverParameter = useMemo(
        () => urlSearchParameters.pipe(map(parameters => parameters.get('popover'))),
        [urlSearchParameters]
    )

    const hoverifier = useMemo(
        () =>
            createHoverifier<HoverContext, HoverMerged, ActionItemAction>({
                pinOptions: {
                    pins: popoverParameter.pipe(
                        filter(value => value === 'pinned'),
                        mapTo(undefined)
                    ),
                    closeButtonClicks: merge(
                        popoverCloses,
                        popoverParameter.pipe(
                            pairwise(),
                            filter(([previous, next]) => previous === 'pinned' && next !== 'pinned'),
                            mapTo(undefined)
                        )
                    ),
                },
                hoverOverlayElements,
                hoverOverlayRerenders: rerenders.pipe(
                    withLatestFrom(hoverOverlayElements, blobElements),
                    map(([, hoverOverlayElement, blobElement]) => ({
                        hoverOverlayElement,
                        relativeElement: blobElement,
                    })),
                    filter(property('relativeElement', isDefined)),
                    // Can't reposition HoverOverlay if it wasn't rendered
                    filter(property('hoverOverlayElement', isDefined))
                ),
                getHover: context =>
                    getHover(getLSPTextDocumentPositionParameters(context, getModeFromPath(context.filePath)), {
                        extensionsController,
                    }),
                getDocumentHighlights: context =>
                    getDocumentHighlights(
                        getLSPTextDocumentPositionParameters(context, getModeFromPath(context.filePath)),
                        { extensionsController }
                    ),
                getActions: context => getHoverActions({ extensionsController, platformContext }, context),
            }),
        [
            popoverParameter,
            popoverCloses,
            hoverOverlayElements,
            rerenders,
            blobElements,
            extensionsController,
            platformContext,
        ]
    )
    useEffect(() => () => hoverifier.unsubscribe(), [hoverifier])

    const customHistoryAction = props.nav
    // Update URL when clicking on a line (which will trigger the line highlighting defined below)
    useObservable(
        useMemo(
            () =>
                codeViewElements.pipe(
                    filter(isDefined),
                    switchMap(codeView => fromEvent<MouseEvent>(codeView, 'click')),
                    // Ignore click events caused by the user selecting text
                    filter(() => !window.getSelection()?.toString()),
                    tap(event => {
                        // Prevent selecting text on shift click (click+drag to select will still work)
                        // Note that this is only called if the selection was empty initially (see above),
                        // so this only clears a selection caused by this click.
                        window.getSelection()!.removeAllRanges()

                        const position = locateTarget(event.target as HTMLElement, domFunctions)
                        if (!position) {
                            return
                        }
                        let query: string | undefined
                        let replace = false
                        if (
                            event.shiftKey &&
                            hoverifier.hoverState.selectedPosition &&
                            hoverifier.hoverState.selectedPosition.line !== undefined
                        ) {
                            // Compare with previous selections (maintained by hoverifier)
                            query = toPositionOrRangeQueryParameter({
                                range: {
                                    start: {
                                        line: Math.min(hoverifier.hoverState.selectedPosition.line, position.line),
                                    },
                                    end: {
                                        line: Math.max(hoverifier.hoverState.selectedPosition.line, position.line),
                                    },
                                },
                            })
                        } else {
                            query = toPositionOrRangeQueryParameter({ position })

                            // Replace the current history entry instead of
                            // adding a new one if the newly selected line is
                            // within 10 lines of the currently selected one.
                            // If the current position is a range a new entry
                            // will always be added.
                            const currentPosition = parseQueryAndHash(location.search, location.hash)
                            replace = Boolean(
                                currentPosition.line &&
                                    !currentPosition.endLine &&
                                    Math.abs(position.line - currentPosition.line) < 11
                            )
                        }

                        const parameters = new URLSearchParams(location.search)
                        parameters.delete('popover')

                        const isClickOnBlankSpace = !('character' in position)
                        if (isClickOnBlankSpace || props.navigateToLineOnAnyClick) {
                            if (customHistoryAction) {
                                const entry: H.LocationDescriptor<unknown> = {
                                    ...location,
                                    search: formatSearchParameters(addLineRangeQueryParameter(parameters, query)),
                                }
                                customHistoryAction(props.history.createHref(entry))
                            } else {
                                updateBrowserHistoryIfChanged(
                                    props.history,
                                    location,
                                    addLineRangeQueryParameter(parameters, query),
                                    replace
                                )
                            }
                        }
                    }),
                    mapTo(undefined)
                ),
            [
                codeViewElements,
                hoverifier.hoverState.selectedPosition,
                location,
                props.history,
                props.navigateToLineOnAnyClick,
                customHistoryAction,
            ]
        )
    )

    // Line highlighting when position in hash changes
    useEffect(() => {
        if (codeViewReference.current) {
            const codeCells = getCodeElementsInRange({
                codeView: codeViewReference.current,
                position: parsedHash,
                getCodeElementFromLineNumber: domFunctions.getCodeElementFromLineNumber,
            })
            // Remove existing highlighting
            for (const selected of codeViewReference.current.querySelectorAll('.selected')) {
                selected.classList.remove('selected')
            }
            for (const { element } of codeCells) {
                // Highlight row
                const row = element.parentElement as HTMLTableRowElement
                row.classList.add('selected')
            }
        }
        // It looks like `parsedHash` is updated _before_ `blobInfo` when
        // navigating between files. That means we have to make this effect
        // dependent on `blobInfo` even if it is not used inside the effect,
        // otherwise the highlighting would not be updated when the new file
        // content is available.
    }, [parsedHash, blobInfo])

    // EXTENSION FEATURES
    // Data source for `viewerUpdates`
    useObservable(
        useMemo(
            () =>
                extensionsController !== null
                    ? combineLatest([
                          blobInfoChanges,
                          // Use the initial position when the document is opened.
                          // Don't want to create new viewers on position change
                          locationPositions.pipe(first()),
                          from(extensionsController.extHostAPI),
                          settingsChanges,
                      ]).pipe(
                          concatMap(([blobInfo, initialPosition, extensionHostAPI]) => {
                              const uri = toURIWithPath(blobInfo)
                              return from(
                                  Promise.all([
                                      // This call should be made before adding viewer, but since
                                      // messages to web worker are handled in order, we can use Promise.all
                                      extensionHostAPI.addTextDocumentIfNotExists({
                                          uri,
                                          languageId: blobInfo.mode,
                                          text: blobInfo.content,
                                      }),
                                      extensionHostAPI.addViewerIfNotExists({
                                          type: 'CodeEditor' as const,
                                          resource: uri,
                                          selections: lprToSelectionsZeroIndexed(initialPosition),
                                          isActive: true,
                                      }),
                                  ])
                              ).pipe(map(([, viewerId]) => ({ viewerId, blobInfo, extensionHostAPI })))
                          }),
                          tap(({ viewerId, blobInfo, extensionHostAPI }) => {
                              const subscriptions = new Subscription()

                              // Cleanup on navigation between/away from viewers
                              subscriptions.add(() => {
                                  extensionHostAPI
                                      .removeViewer(viewerId)
                                      .catch(error => logger.error('Error removing viewer from extension host', error))
                              })

                              viewerUpdates.next({ viewerId, blobInfo, extensionHostAPI, subscriptions })
                          }),
                          mapTo(undefined)
                      )
                    : EMPTY,
            [blobInfoChanges, locationPositions, extensionsController, settingsChanges, viewerUpdates]
        )
    )

    // Hoverify
    useObservable(
        useMemo(
            () =>
                viewerUpdates.pipe(
                    filter(isDefined),
                    tap(viewerData => {
                        const subscription = hoverifier.hoverify({
                            positionEvents: codeViewElements.pipe(
                                filter(isDefined),
                                findPositionsFromEvents({ domFunctions })
                            ),
                            positionJumps: locationPositions.pipe(
                                withLatestFrom(
                                    codeViewElements.pipe(filter(isDefined)),
                                    blobElements.pipe(filter(isDefined))
                                ),
                                map(([position, codeView, scrollElement]) => ({
                                    position,
                                    // locationPositions is derived from componentUpdates,
                                    // so these elements are guaranteed to have been rendered.
                                    codeView,
                                    scrollElement,
                                }))
                            ),
                            resolveContext: () => {
                                const { repoName, revision, commitID, filePath } = viewerData.blobInfo
                                return {
                                    repoName,
                                    revision,
                                    commitID,
                                    filePath,
                                }
                            },
                            dom: domFunctions,
                        })
                        viewerData.subscriptions.add(() => subscription.unsubscribe())
                    }),
                    mapTo(undefined)
                ),
            [hoverifier, viewerUpdates, codeViewElements, blobElements, locationPositions]
        )
    )

    // Warm cache for references panel. Eventually display a loading indicator
    useObservable(
        useMemo(
            () =>
                extensionsController !== null ? haveInitialExtensionsLoaded(extensionsController.extHostAPI) : EMPTY,
            [extensionsController]
        )
    )

    // Passed to HoverOverlay
    const hoverState: Readonly<HoverState<HoverContext, HoverMerged, ActionItemAction>> =
        useObservable(hoverifier.hoverStateUpdates) || {}

    const pinOptions = useMemo<PinOptions>(
        () => ({
            showCloseButton: true,
            onCloseButtonClick: nextPopoverClose,
            onCopyLinkButtonClick: async () => {
                const line = hoverifier.hoverState.hoveredToken?.line
                const character = hoverifier.hoverState.hoveredToken?.character
                if (line === undefined || character === undefined) {
                    return
                }
                const point = { line, character }
                const range = { start: point, end: point }
                const context = { position: point, range }
                const search = new URLSearchParams(location.search)
                search.set('popover', 'pinned')
                updateBrowserHistoryIfChanged(
                    props.history,
                    location,
                    addLineRangeQueryParameter(search, toPositionOrRangeQueryParameter(context))
                )
                await navigator.clipboard.writeText(window.location.href)
            },
        }),
        [
            hoverifier.hoverState.hoveredToken?.line,
            hoverifier.hoverState.hoveredToken?.character,
            location,
            nextPopoverClose,
            props.history,
        ]
    )

    // Add top and bottom spacers to improve code readability.
    useEffect(() => {
        const subscription = codeViewElements.subscribe(codeView => {
            if (codeView) {
                const table = codeView.firstElementChild as HTMLTableElement
                const firstRow = table.rows[0]
                const lastRow = table.rows[table.rows.length - 1]

                if (firstRow) {
                    for (const cell of firstRow.cells) {
                        if (!cell.querySelector('.top-spacer')) {
                            const spacer = document.createElement('div')
                            spacer.classList.add('top-spacer')
                            cell.prepend(spacer)
                        }
                    }
                }

                if (lastRow) {
                    for (const cell of lastRow.cells) {
                        if (!cell.querySelector('.bottom-spacer')) {
                            const spacer = document.createElement('div')
                            spacer.classList.add('bottom-spacer')
                            cell.append(spacer)
                        }
                    }
                }
            }
        })

        return () => {
            subscription.unsubscribe()
        }
    }, [codeViewElements])

    // Add the `.clickable-row` CSS class to all rows to give visual hints that they're clickable.
    useLayoutEffect(() => {
        if (!props.navigateToLineOnAnyClick) {
            return
        }

        const subscription = codeViewElements.subscribe(codeView => {
            if (codeView) {
                const table = codeView.firstElementChild as HTMLTableElement
                for (const row of table.rows) {
                    if (row.cells.length === 0) {
                        continue
                    }
                    row.className = styles.clickableRow
                }
            }
        })

        return () => {
            subscription.unsubscribe()
        }
    }, [codeViewElements, props.navigateToLineOnAnyClick])

    const logEventOnCopy = useCallback(() => {
        props.telemetryService.log(...codeCopiedEvent('blob'))
    }, [props.telemetryService])

    return (
        <>
            <div
                data-testid={dataTestId}
                className={classNames(props.className, styles.blob)}
                ref={nextBlobElement}
                tabIndex={-1}
                role={role}
                aria-label={ariaLabel}
            >
                <Code
                    className={classNames('test-blob', styles.blobCode, props.wrapCode && styles.blobCodeWrapped)}
                    ref={nextCodeViewElement}
                    onCopy={logEventOnCopy}
                    dangerouslySetInnerHTML={{
                        __html: blobInfo.html,
                    }}
                />
                {!props.navigateToLineOnAnyClick && hoverState.hoverOverlayProps && extensionsController !== null && (
                    <WebHoverOverlay
                        {...props}
                        {...hoverState.hoverOverlayProps}
                        nav={url => (props.nav ? props.nav(url) : props.history.push(url))}
                        hoveredTokenElement={hoverState.hoveredTokenElement}
                        hoverRef={nextOverlayElement}
                        pinOptions={pinOptions}
                        extensionsController={extensionsController}
                    />
                )}

                <BlameColumn
                    isBlameVisible={props.isBlameVisible}
                    blameHunks={props.blameHunks}
                    codeViewElements={codeViewElements}
                    history={props.history}
                    isLightTheme={isLightTheme}
                />
            </div>
        </>
    )
}

/**
 * Adds an entry to the browser history only if new search parameters differ
 * from the current ones. This prevents adding a new entry when e.g. the user
 * clicks the same line multiple times.
 */
export function updateBrowserHistoryIfChanged(
    history: H.History,
    location: H.Location,
    newSearchParameters: URLSearchParams,
    /** If set to true replace the current history entry instead of adding a new one. */
    replace: boolean = false
): void {
    const currentSearchParameters = [...new URLSearchParams(location.search).entries()]

    // Update history if the number of search params changes or if any parameter
    // value changes. This will also work for file position changes, which are
    // encoded as parameter without a value. The old file position will be a
    // non-existing key in the new search parameters and thus return `null`
    // (whereas it returns an empty string in the current search parameters).
    const needsUpdate =
        currentSearchParameters.length !== [...newSearchParameters.keys()].length ||
        currentSearchParameters.some(([key, value]) => newSearchParameters.get(key) !== value)

    if (needsUpdate) {
        const entry = {
            ...location,
            search: formatSearchParameters(newSearchParameters),
        }
        if (replace) {
            history.replace(entry)
        } else {
            history.push(entry)
        }
    }
}

export function getLSPTextDocumentPositionParameters(
    position: HoveredToken & RepoSpec & RevisionSpec & FileSpec & ResolvedRevisionSpec,
    mode: string
): RepoSpec & RevisionSpec & ResolvedRevisionSpec & FileSpec & UIPositionSpec & ModeSpec {
    return {
        repoName: position.repoName,
        filePath: position.filePath,
        commitID: position.commitID,
        revision: position.revision,
        mode,
        position,
    }
}
