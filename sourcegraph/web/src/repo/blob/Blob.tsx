import { createHoverifier, findPositionsFromEvents, HoveredToken, HoverState } from '@sourcegraph/codeintellify'
import { getCodeElementsInRange, locateTarget } from '@sourcegraph/codeintellify/lib/token_position'
import { TextDocumentDecoration } from '@sourcegraph/extension-api-types'
import * as H from 'history'
import { isEqual, pick } from 'lodash'
import * as React from 'react'
import { combineLatest, fromEvent, merge, Observable, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, filter, map, share, switchMap, withLatestFrom } from 'rxjs/operators'
import { ActionItemProps } from '../../../../shared/src/actions/ActionItem'
import { decorationStyleForTheme } from '../../../../shared/src/api/client/services/decoration'
import { HoverMerged } from '../../../../shared/src/api/client/types/hover'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import { getHoverActions } from '../../../../shared/src/hover/actions'
import { HoverContext, HoverOverlay } from '../../../../shared/src/hover/HoverOverlay'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { SettingsCascadeProps } from '../../../../shared/src/settings/settings'
import { asError, ErrorLike, isErrorLike } from '../../../../shared/src/util/errors'
import { isDefined, propertyIsDefined } from '../../../../shared/src/util/types'
import {
    AbsoluteRepoFile,
    FileSpec,
    LineOrPositionOrRange,
    lprToSelectionsZeroIndexed,
    ModeSpec,
    parseHash,
    PositionSpec,
    RenderMode,
    RepoSpec,
    ResolvedRevSpec,
    RevSpec,
    toPositionOrRangeHash,
} from '../../../../shared/src/util/url'
import { getHover } from '../../backend/features'
import { isDiscussionsEnabled } from '../../discussions'
import { ThemeProps } from '../../theme'
import { DiscussionsGutterOverlay } from './discussions/DiscussionsGutterOverlay'
import { LineDecorationAttachment } from './LineDecorationAttachment'

/**
 * toPortalID builds an ID that will be used for the {@link LineDecorationAttachment} portal containers.
 */
const toPortalID = (line: number) => `line-decoration-attachment-${line}`

interface BlobProps
    extends AbsoluteRepoFile,
        ModeSpec,
        SettingsCascadeProps,
        PlatformContextProps,
        ExtensionsControllerProps,
        ThemeProps {
    /** The raw content of the blob. */
    content: string

    /** The trusted syntax-highlighted code as HTML */
    html: string

    location: H.Location
    history: H.History
    className: string
    wrapCode: boolean
    renderMode: RenderMode
}

interface BlobState extends HoverState<HoverContext, HoverMerged, ActionItemProps> {
    /** The desired position of the discussions gutter overlay */
    discussionsGutterOverlayPosition?: { left: number; top: number }

    /**
     * lineDecorationAttachmentIDs is a map from line numbers with portal nodes created to portal IDs. It's used to
     * render the portals for {@link LineDecorationAttachment}. The line numbers are taken from the blob so they
     * are 1-indexed.
     */
    lineDecorationAttachmentIDs: { [key: number]: string }

    /** The decorations to display in the blob. */
    decorationsOrError?: TextDocumentDecoration[] | null | ErrorLike
}

const domFunctions = {
    getCodeElementFromTarget: (target: HTMLElement): HTMLTableCellElement | null => {
        // If the target is part of the line decoration attachment, return null.
        if (
            target.classList.contains('line-decoration-attachment') ||
            target.classList.contains('line-decoration-attachment__contents')
        ) {
            return null
        }

        const row = target.closest('tr')
        if (!row) {
            return null
        }
        return row.cells[1]
    },
    getCodeElementFromLineNumber: (codeView: HTMLElement, line: number): HTMLTableCellElement | null => {
        const table = codeView.firstElementChild as HTMLTableElement
        const row = table.rows[line - 1]
        if (!row) {
            return null
        }
        return row.cells[1]
    },
    getLineNumberFromCodeElement: (codeCell: HTMLElement): number => {
        const row = codeCell.closest('tr')
        if (!row) {
            throw new Error('Could not find closest row for codeCell')
        }
        const numberCell = row.cells[0]
        if (!numberCell || !numberCell.dataset.line) {
            throw new Error('Could not find line number')
        }
        return parseInt(numberCell.dataset.line, 10)
    },
}

export class Blob extends React.Component<BlobProps, BlobState> {
    /** Emits with the latest Props on every componentDidUpdate and on componentDidMount */
    private componentUpdates = new Subject<BlobProps>()

    /** Emits whenever the ref callback for the code element is called */
    private codeViewElements = new Subject<HTMLElement | null>()
    private nextCodeViewElement = (element: HTMLElement | null) => this.codeViewElements.next(element)

    /** Emits whenever the ref callback for the blob element is called */
    private blobElements = new Subject<HTMLElement | null>()
    private nextBlobElement = (element: HTMLElement | null) => this.blobElements.next(element)

    /** Emits whenever the ref callback for the hover element is called */
    private hoverOverlayElements = new Subject<HTMLElement | null>()
    private nextOverlayElement = (element: HTMLElement | null) => this.hoverOverlayElements.next(element)

    /** Emits when the close button was clicked */
    private closeButtonClicks = new Subject<MouseEvent>()
    private nextCloseButtonClick = (event: MouseEvent) => this.closeButtonClicks.next(event)

    /** Subscriptions to be disposed on unmout */
    private subscriptions = new Subscription()

    constructor(props: BlobProps) {
        super(props)
        this.state = {
            lineDecorationAttachmentIDs: {},
        }

        /** Emits parsed positions found in the URL */
        const locationPositions: Observable<LineOrPositionOrRange> = this.componentUpdates.pipe(
            map(props => parseHash(props.location.hash)),
            distinctUntilChanged((a, b) => isEqual(a, b)),
            share()
        )

        const hoverifier = createHoverifier<
            RepoSpec & RevSpec & FileSpec & ResolvedRevSpec,
            HoverMerged,
            ActionItemProps
        >({
            closeButtonClicks: this.closeButtonClicks,
            hoverOverlayElements: this.hoverOverlayElements,
            hoverOverlayRerenders: this.componentUpdates.pipe(
                withLatestFrom(this.hoverOverlayElements, this.blobElements),
                // After componentDidUpdate, the blob element is guaranteed to have been rendered
                map(([, hoverOverlayElement, blobElement]) => ({ hoverOverlayElement, relativeElement: blobElement! })),
                // Can't reposition HoverOverlay if it wasn't rendered
                filter(propertyIsDefined('hoverOverlayElement'))
            ),
            getHover: position => getHover(this.getLSPTextDocumentPositionParams(position), this.props),
            getActions: context => getHoverActions(this.props, context),
        })
        this.subscriptions.add(hoverifier)

        const resolveContext = () => ({
            repoName: this.props.repoName,
            rev: this.props.rev,
            commitID: this.props.commitID,
            filePath: this.props.filePath,
        })
        this.subscriptions.add(
            hoverifier.hoverify({
                positionEvents: this.codeViewElements.pipe(
                    filter(isDefined),
                    findPositionsFromEvents(domFunctions)
                ),
                positionJumps: locationPositions.pipe(
                    withLatestFrom(this.codeViewElements, this.blobElements),
                    map(([position, codeView, scrollElement]) => ({
                        position,
                        // locationPositions is derived from componentUpdates,
                        // so these elements are guaranteed to have been rendered.
                        codeView: codeView!,
                        scrollElement: scrollElement!,
                    }))
                ),
                resolveContext,
                dom: domFunctions,
            })
        )
        this.subscriptions.add(
            hoverifier.hoverStateUpdates.subscribe(update => {
                this.setState(update)
            })
        )

        // When clicking a line, update the URL (which will in turn trigger a highlight of the line)
        this.subscriptions.add(
            this.codeViewElements
                .pipe(
                    filter(isDefined),
                    switchMap(codeView => fromEvent<MouseEvent>(codeView, 'click')),
                    // Ignore click events caused by the user selecting text
                    filter(() => window.getSelection().toString() === '')
                )
                .subscribe(event => {
                    // Prevent selecting text on shift click (click+drag to select will still work)
                    // Note that this is only called if the selection was empty initially (see above),
                    // so this only clears a selection caused by this click.
                    window.getSelection().removeAllRanges()

                    const position = locateTarget(event.target as HTMLElement, domFunctions)
                    let hash: string
                    if (
                        position &&
                        event.shiftKey &&
                        this.state.selectedPosition &&
                        this.state.selectedPosition.line !== undefined
                    ) {
                        hash = toPositionOrRangeHash({
                            range: {
                                start: {
                                    line: Math.min(this.state.selectedPosition.line, position.line),
                                },
                                end: {
                                    line: Math.max(this.state.selectedPosition.line, position.line),
                                },
                            },
                        })
                    } else {
                        hash = toPositionOrRangeHash({ position })
                    }

                    if (!hash.startsWith('#')) {
                        hash = '#' + hash
                    }

                    this.props.history.push({ ...this.props.location, hash })
                })
        )

        // LOCATION CHANGES
        this.subscriptions.add(
            locationPositions.pipe(withLatestFrom(this.codeViewElements)).subscribe(([position, codeView]) => {
                codeView = codeView! // locationPositions is derived from componentUpdates, so this is guaranteed to exist
                const codeCells = getCodeElementsInRange({
                    codeView,
                    position,
                    getCodeElementFromLineNumber: domFunctions.getCodeElementFromLineNumber,
                })
                // Remove existing highlighting
                for (const selected of codeView.querySelectorAll('.selected')) {
                    selected.classList.remove('selected')
                }
                for (const { line, element } of codeCells) {
                    this.createLineDecorationAttachmentDOMNode(line, element)
                    // Highlight row
                    const row = element.parentElement as HTMLTableRowElement
                    row.classList.add('selected')
                }

                // Update overlay position for discussions gutter icon.
                if (codeCells.length > 0) {
                    const blobBounds = codeView.parentElement!.getBoundingClientRect()
                    const row = codeCells[0].element.parentElement as HTMLTableRowElement
                    const targetBounds = row.cells[0].getBoundingClientRect()
                    const left = targetBounds.left - blobBounds.left
                    const top = targetBounds.top + codeView.parentElement!.scrollTop - blobBounds.top
                    this.setState({ discussionsGutterOverlayPosition: { left, top } })
                }
            })
        )

        /** Emits when the URL's target blob (repository, revision, path, and content) changes. */
        const modelChanges: Observable<
            AbsoluteRepoFile & ModeSpec & Pick<BlobProps, 'content' | 'isLightTheme'>
        > = this.componentUpdates.pipe(
            map(props => pick(props, 'repoName', 'rev', 'commitID', 'filePath', 'mode', 'content', 'isLightTheme')),
            distinctUntilChanged((a, b) => isEqual(a, b)),
            share()
        )

        // Update the Sourcegraph extensions model to reflect the current file.
        this.subscriptions.add(
            combineLatest(modelChanges, locationPositions).subscribe(([model, pos]) => {
                this.props.extensionsController.services.model.model.next({
                    ...this.props.extensionsController.services.model.model.value,
                    visibleViewComponents: [
                        {
                            type: 'textEditor' as 'textEditor',
                            item: {
                                uri: `git://${model.repoName}?${model.commitID}#${model.filePath}`,
                                languageId: model.mode,
                                text: model.content,
                            },
                            selections: lprToSelectionsZeroIndexed(pos),
                            isActive: true,
                        },
                    ],
                })
            })
        )

        /** Decorations */
        let lastModel: (AbsoluteRepoFile & ModeSpec) | undefined
        const decorations: Observable<TextDocumentDecoration[] | null> = combineLatest(modelChanges).pipe(
            switchMap(([model]) => {
                const modelChanged = !isEqual(model, lastModel)
                lastModel = model // record so we can compute modelChanged

                // Only clear decorations if the model changed. If only the extensions changed, keep
                // the old decorations until the new ones are available, to avoid UI jitter.
                return merge(
                    modelChanged ? [null] : [],
                    this.props.extensionsController.services.textDocumentDecoration.getDecorations({
                        uri: `git://${model.repoName}?${model.commitID}#${model.filePath}`,
                    })
                )
            }),
            share()
        )
        this.subscriptions.add(
            decorations
                .pipe(catchError(error => [asError(error)]))
                .subscribe(decorationsOrError => this.setState({ decorationsOrError }))
        )

        /** Render decorations. */
        let decoratedElements: HTMLElement[] = []
        this.subscriptions.add(
            combineLatest(
                decorations.pipe(
                    map(decorations => decorations || []),
                    catchError(error => {
                        console.error(error)

                        // Treat decorations error as empty decorations.
                        return [[] as TextDocumentDecoration[]]
                    })
                ),
                this.codeViewElements
            ).subscribe(([decorations, codeView]) => {
                if (codeView) {
                    if (decoratedElements) {
                        // Clear previous decorations.
                        for (const element of decoratedElements) {
                            element.style.backgroundColor = null
                            element.style.border = null
                            element.style.borderColor = null
                            element.style.borderWidth = null
                        }
                    }

                    for (const decoration of decorations) {
                        const line = decoration.range.start.line + 1
                        const codeCell = domFunctions.getCodeElementFromLineNumber(codeView, line)
                        if (!codeCell) {
                            continue
                        }
                        const row = codeCell.parentElement as HTMLTableRowElement
                        let decorated = false
                        const style = decorationStyleForTheme(decoration, this.props.isLightTheme)
                        if (style.backgroundColor) {
                            row.style.backgroundColor = style.backgroundColor
                            decorated = true
                        }
                        if (style.border) {
                            row.style.border = style.border
                            decorated = true
                        }
                        if (style.borderColor) {
                            row.style.borderColor = style.borderColor
                            decorated = true
                        }
                        if (style.borderWidth) {
                            row.style.borderWidth = style.borderWidth
                            decorated = true
                        }
                        if (decorated) {
                            decoratedElements.push(row)
                        }

                        if (decoration.after) {
                            const codeCell = row.cells[1]!
                            this.createLineDecorationAttachmentDOMNode(line, codeCell)
                        }
                    }
                } else {
                    decoratedElements = []
                }
            })
        )
    }

    private getLSPTextDocumentPositionParams(
        position: HoveredToken & RepoSpec & RevSpec & FileSpec & ResolvedRevSpec
    ): RepoSpec & RevSpec & ResolvedRevSpec & FileSpec & PositionSpec & ModeSpec {
        return {
            repoName: position.repoName,
            filePath: position.filePath,
            commitID: position.commitID,
            rev: position.rev,
            mode: this.props.mode,
            position,
        }
    }

    /**
     * Appends a {@link LineDecorationAttachment} portal DOM node to the given code cell if it doesn't contain one
     * already.
     *
     * @param line 1-indexed line number
     * @param codeCell The `<td class="code">` element
     */
    private createLineDecorationAttachmentDOMNode(line: number, codeCell: HTMLElement): void {
        if (codeCell.querySelector('.line-decoration-attachment-portal')) {
            return
        }
        const portalNode = document.createElement('span')

        const id = toPortalID(line)
        portalNode.id = id
        portalNode.classList.add('line-decoration-attachment-portal')

        codeCell.appendChild(portalNode)

        this.setState(state => ({
            lineDecorationAttachmentIDs: {
                ...state.lineDecorationAttachmentIDs,
                [line]: id,
            },
        }))
    }

    public componentDidMount(): void {
        this.componentUpdates.next(this.props)
    }

    public shouldComponentUpdate(nextProps: Readonly<BlobProps>, nextState: Readonly<BlobState>): boolean {
        return !isEqual(this.props, nextProps) || !isEqual(this.state, nextState)
    }

    public componentDidUpdate(): void {
        this.componentUpdates.next(this.props)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): React.ReactNode {
        return (
            <div className={`blob ${this.props.className}`} ref={this.nextBlobElement}>
                <code
                    className={`blob__code ${this.props.wrapCode ? ' blob__code--wrapped' : ''} e2e-blob`}
                    ref={this.nextCodeViewElement}
                    dangerouslySetInnerHTML={{ __html: this.props.html }}
                />
                {this.state.hoverOverlayProps && (
                    <HoverOverlay
                        {...this.state.hoverOverlayProps}
                        hoverRef={this.nextOverlayElement}
                        onCloseButtonClick={this.nextCloseButtonClick}
                        extensionsController={this.props.extensionsController}
                        platformContext={this.props.platformContext}
                        location={this.props.location}
                    />
                )}
                {this.state.decorationsOrError &&
                    !isErrorLike(this.state.decorationsOrError) &&
                    this.state.decorationsOrError
                        .filter(d => !!d.after && this.state.lineDecorationAttachmentIDs[d.range.start.line + 1])
                        .map(d => {
                            const line = d.range.start.line + 1
                            return (
                                <LineDecorationAttachment
                                    key={this.state.lineDecorationAttachmentIDs[line]}
                                    portalID={this.state.lineDecorationAttachmentIDs[line]}
                                    line={line}
                                    attachment={d.after!}
                                    {...this.props}
                                />
                            )
                        })}
                {isDiscussionsEnabled(this.props.settingsCascade) &&
                    this.state.selectedPosition &&
                    this.state.selectedPosition.line !== undefined && (
                        <DiscussionsGutterOverlay
                            overlayPosition={this.state.discussionsGutterOverlayPosition}
                            selectedPosition={this.state.selectedPosition}
                            {...this.props}
                        />
                    )}
            </div>
        )
    }
}
