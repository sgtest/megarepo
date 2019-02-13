import {
    ContextResolver,
    createHoverifier,
    DiffPart,
    DOMFunctions,
    findPositionsFromEvents,
    Hoverifier,
    HoverState,
    PositionAdjuster,
} from '@sourcegraph/codeintellify'
import { propertyIsDefined } from '@sourcegraph/codeintellify/lib/helpers'
import { Selection } from '@sourcegraph/extension-api-types'
import * as H from 'history'
import { isEqual } from 'lodash'
import * as React from 'react'
import { createPortal, render } from 'react-dom'
import { animationFrameScheduler, combineLatest, fromEvent, Observable, of, Subject, Subscription } from 'rxjs'
import {
    catchError,
    distinctUntilChanged,
    filter,
    map,
    mergeMap,
    observeOn,
    startWith,
    switchMap,
    withLatestFrom,
} from 'rxjs/operators'
import { registerHighlightContributions } from '../../../../../shared/src/highlight/contributions'

import { ActionItemProps } from '../../../../../shared/src/actions/ActionItem'
import { Model, ViewComponentData } from '../../../../../shared/src/api/client/model'
import { HoverMerged } from '../../../../../shared/src/api/client/types/hover'
import { ExtensionsControllerProps } from '../../../../../shared/src/extensions/controller'
import { getHoverActions, registerHoverContributions } from '../../../../../shared/src/hover/actions'
import { HoverContext, HoverOverlay } from '../../../../../shared/src/hover/HoverOverlay'
import { getModeFromPath } from '../../../../../shared/src/languages'
import { PlatformContextProps } from '../../../../../shared/src/platform/context'
import { TelemetryContext } from '../../../../../shared/src/telemetry/telemetryContext'
import {
    FileSpec,
    lprToSelectionsZeroIndexed,
    parseHash,
    PositionSpec,
    RepoSpec,
    ResolvedRevSpec,
    RevSpec,
    toRootURI,
    toURIWithPath,
    ViewStateSpec,
} from '../../../../../shared/src/util/url'
import { sendMessage } from '../../browser/runtime'
import { isInPage } from '../../context'
import { ERPRIVATEREPOPUBLICSOURCEGRAPHCOM } from '../../shared/backend/errors'
import { createLSPFromExtensions, toTextDocumentIdentifier } from '../../shared/backend/lsp'
import { ButtonProps, CodeViewToolbar } from '../../shared/components/CodeViewToolbar'
import { resolveRev, retryWhenCloneInProgressError } from '../../shared/repo/backend'
import { eventLogger, sourcegraphUrl } from '../../shared/util/context'
import { bitbucketServerCodeHost } from '../bitbucket/code_intelligence'
import { githubCodeHost } from '../github/code_intelligence'
import { gitlabCodeHost } from '../gitlab/code_intelligence'
import { phabricatorCodeHost } from '../phabricator/code_intelligence'
import { fetchFileContents, findCodeViews } from './code_views'
import { applyDecorations, initializeExtensions } from './extensions'
import { injectViewContextOnSourcegraph } from './external_links'

registerHighlightContributions()

/**
 * Defines a type of code view a given code host can have. It tells us how to
 * look for the code view and how to do certain things when we find it.
 */
export interface CodeView {
    /** A selector used by `document.querySelectorAll` to find the code view. */
    selector: string
    /** The DOMFunctions for the code view. */
    dom: DOMFunctions
    /**
     * Finds or creates a DOM element where we should inject the
     * `CodeViewToolbar`. This function is responsible for ensuring duplicate
     * mounts aren't created.
     */
    getToolbarMount?: (codeView: HTMLElement, part?: DiffPart) => HTMLElement
    /**
     * Resolves the file info for a given code view. It returns an observable
     * because some code hosts need to resolve this asynchronously. The
     * observable should only emit once.
     */
    resolveFileInfo: (codeView: HTMLElement) => Observable<FileInfo>
    /**
     * In some situations, we need to be able to adjust the position going into
     * and coming out of codeintellify. For example, Phabricator converts tabs
     * to spaces in it's DOM.
     */
    adjustPosition?: PositionAdjuster<RepoSpec & RevSpec & FileSpec & ResolvedRevSpec>
    /** Props for styling the buttons in the `CodeViewToolbar`. */
    toolbarButtonProps?: ButtonProps

    isDiff?: boolean
}

export type CodeViewWithOutSelector = Pick<CodeView, Exclude<keyof CodeView, 'selector'>>

export interface CodeViewResolver {
    selector: string
    resolveCodeView: (elem: HTMLElement) => CodeViewWithOutSelector | null
}

interface OverlayPosition {
    top: number
    left: number
}

/**
 * A function that gets the mount location for elements being mounted to the DOM.
 */
export type MountGetter = () => HTMLElement

/**
 * The context the code host is in on the current page.
 */
export type CodeHostContext = RepoSpec & Partial<RevSpec>

/** Information for adding code intelligence to code views on arbitrary code hosts. */
export interface CodeHost {
    /**
     * The name of the code host. This will be added as a className to the overlay mount.
     */
    name: string

    /**
     * Basic contextual information for the current code host.
     */
    getContext?: () => CodeHostContext
    /**
     * The mount location for the contextual link to Sourcegraph.
     */
    getViewContextOnSourcegraphMount?: () => HTMLElement | null
    /**
     * Optional class name for the contextual link to Sourcegraph.
     */
    contextButtonClassName?: string

    /**
     * Checks to see if the current context the code is running in is within
     * the given code host.
     */
    check: () => Promise<boolean> | boolean

    /**
     * Gets the mount location for the hover overlay. Defaults to a created `<div>`
     * that is appended to `document.body`. Use this control to remove the
     * tooltip when the portion of the page containing the code views is removed
     * (e.g. from a soft page reload).
     */
    getOverlayMount?: () => HTMLElement | null

    /**
     * The list of types of code views to try to annotate.
     */
    codeViews?: CodeView[]

    /**
     * Resolve `CodeView`s from the DOM. This is useful when each code view type
     * doesn't have a distinct selector for
     */
    codeViewResolver?: CodeViewResolver

    /**
     * Adjust the position of the hover overlay. Useful for fixed headers or other
     * elements that throw off the position of the tooltip within the relative
     * element.
     */
    adjustOverlayPosition?: (position: OverlayPosition) => OverlayPosition

    // Extensions related input

    /**
     * Get the DOM element where we'll mount the command palette for extensions.
     */
    getCommandPaletteMount?: MountGetter

    /**
     * Get the DOM element where we'll mount the small global debug menu for extensions in the bottom right.
     */
    getGlobalDebugMount?: MountGetter

    /** Construct the URL to the specified file. */
    urlToFile?: (
        location: RepoSpec & RevSpec & FileSpec & Partial<PositionSpec> & Partial<ViewStateSpec> & { part?: DiffPart }
    ) => string

    /** Returns a stream representing the selections in the current code view */
    selectionsChanges?: () => Observable<Selection[]>
}

export interface FileInfo {
    /**
     * The path for the repo the file belongs to. If a `baseRepoName` is provided, this value
     * is treated as the head repo name.
     */
    repoName: string
    /**
     * The path for the file path for a given `codeView`. If a `baseFilePath` is provided, this value
     * is treated as the head file path.
     */
    filePath: string
    /**
     * The commit that the code view is at. If a `baseCommitID` is provided, this value is treated
     * as the head commit ID.
     */
    commitID: string
    /**
     * The revision the code view is at. If a `baseRev` is provided, this value is treated as the head rev.
     */
    rev?: string
    /**
     * The repo name for the BASE side of a diff. This is useful for Phabricator
     * staging areas since they are separate repos.
     */
    baseRepoName?: string
    /**
     * The base file path.
     */
    baseFilePath?: string
    /**
     * Commit ID for the BASE side of the diff.
     */
    baseCommitID?: string
    /**
     * Revision for the BASE side of the diff.
     */
    baseRev?: string

    headHasFileContents?: boolean
    baseHasFileContents?: boolean

    content?: string
    baseContent?: string
}

/**
 * Prepares the page for code intelligence. It creates the hoverifier, injects
 * and mounts the hover overlay and then returns the hoverifier.
 *
 * @param codeHost
 */
function initCodeIntelligence(
    codeHost: CodeHost
): {
    hoverifier: Hoverifier<RepoSpec & RevSpec & FileSpec & ResolvedRevSpec, HoverMerged, ActionItemProps>
    controllers: ExtensionsControllerProps & PlatformContextProps
} {
    const {
        platformContext,
        extensionsController,
    }: PlatformContextProps & ExtensionsControllerProps = initializeExtensions(codeHost)

    const { getHover } = createLSPFromExtensions(extensionsController)

    /** Emits when the close button was clicked */
    const closeButtonClicks = new Subject<MouseEvent>()
    const nextCloseButtonClick = (event: MouseEvent) => closeButtonClicks.next(event)

    /** Emits whenever the ref callback for the hover element is called */
    const hoverOverlayElements = new Subject<HTMLElement | null>()
    const nextOverlayElement = (element: HTMLElement | null) => hoverOverlayElements.next(element)

    const relativeElement = document.body

    const containerComponentUpdates = new Subject<void>()

    registerHoverContributions({ extensionsController, platformContext, history: H.createBrowserHistory() })

    const hoverifier = createHoverifier<RepoSpec & RevSpec & FileSpec & ResolvedRevSpec, HoverMerged, ActionItemProps>({
        closeButtonClicks,
        hoverOverlayElements,
        hoverOverlayRerenders: containerComponentUpdates.pipe(
            withLatestFrom(hoverOverlayElements),
            map(([, hoverOverlayElement]) => ({ hoverOverlayElement, relativeElement })),
            filter(propertyIsDefined('hoverOverlayElement'))
        ),
        getHover: ({ line, character, part, ...rest }) =>
            getHover({ ...rest, position: { line, character } }).pipe(
                map(hover => (hover ? (hover as HoverMerged) : hover))
            ),
        getActions: context => getHoverActions({ extensionsController, platformContext }, context),
    })

    const classNames = ['hover-overlay-mount', `hover-overlay-mount__${codeHost.name}`]

    const createOverlayContainerMount = () => {
        const overlayMount = document.createElement('div')
        overlayMount.style.height = '0px'
        overlayMount.classList.add('overlay-mount-container')
        document.body.appendChild(overlayMount)
        return overlayMount
    }

    const overlayContainerMount = document.querySelector('.overlay-mount-container') || createOverlayContainerMount()

    const getOverlayMount = (): HTMLElement => {
        let mount: HTMLElement | null = document.querySelector('.sg-overlay-mount')
        if (mount) {
            mount.parentElement!.removeChild(mount)
        }

        if (codeHost.getOverlayMount) {
            mount = codeHost.getOverlayMount()
        }

        if (!mount) {
            mount = document.createElement('div')
            overlayContainerMount.appendChild(mount)
        }

        mount.classList.add('sg-overlay-mount')
        for (const className of classNames) {
            mount.classList.add(className)
        }

        return mount
    }

    class HoverOverlayContainer extends React.Component<{}, HoverState<HoverContext, HoverMerged, ActionItemProps>> {
        private portal: HTMLElement | null = null

        private observer: MutationObserver

        constructor(props: {}) {
            super(props)
            this.state = hoverifier.hoverState
            hoverifier.hoverStateUpdates.subscribe(update => this.setState(update))

            this.observer = new MutationObserver(mutations => {
                for (const mutation of mutations) {
                    if (mutation.type === 'childList') {
                        for (const removedNode of mutation.removedNodes) {
                            if (removedNode.contains(removedNode)) {
                                nextCloseButtonClick(new MouseEvent('click'))
                            }
                        }
                    }
                }
            })
        }
        public componentDidMount(): void {
            containerComponentUpdates.next()
            if (this.portal) {
                this.observer.observe(this.portal.parentElement!, { childList: true })
            }
        }
        public componentDidUpdate(): void {
            if (!this.portal || !document.body.contains(this.portal)) {
                this.portal = getOverlayMount()
                this.observer.observe(this.portal.parentElement!, { childList: true })
            }

            containerComponentUpdates.next()
        }
        public render(): JSX.Element | null {
            const hoverOverlayProps = this.getHoverOverlayProps()
            return hoverOverlayProps && this.portal
                ? createPortal(
                      <HoverOverlay
                          {...hoverOverlayProps}
                          hoverRef={nextOverlayElement}
                          extensionsController={extensionsController!}
                          platformContext={platformContext!}
                          location={H.createLocation(window.location)}
                          onCloseButtonClick={nextCloseButtonClick}
                      />,
                      this.portal
                  )
                : null
        }
        private getHoverOverlayProps(): HoverState<HoverContext, HoverMerged, ActionItemProps>['hoverOverlayProps'] {
            if (!this.state.hoverOverlayProps) {
                return undefined
            }

            let { overlayPosition, ...rest } = this.state.hoverOverlayProps
            if (overlayPosition && codeHost.adjustOverlayPosition) {
                overlayPosition = codeHost.adjustOverlayPosition(overlayPosition)
            }

            return {
                ...rest,
                overlayPosition,
            }
        }
    }

    render(
        <TelemetryContext.Provider value={eventLogger}>
            <HoverOverlayContainer />
        </TelemetryContext.Provider>,
        overlayContainerMount
    )

    return { hoverifier, controllers: { platformContext, extensionsController } }
}

/**
 * ResolvedCodeView attaches an actual code view DOM element that was found on
 * the page to the CodeView type being passed around by this file.
 */
export interface ResolvedCodeView extends CodeViewWithOutSelector {
    /** The code view DOM element. */
    codeView: HTMLElement
}

function handleCodeHost(codeHost: CodeHost): Subscription {
    const {
        hoverifier,
        controllers: { platformContext, extensionsController },
    } = initCodeIntelligence(codeHost)

    const subscriptions = new Subscription()

    subscriptions.add(hoverifier)

    const ensureRepoExists = (context: CodeHostContext) =>
        resolveRev(context).pipe(
            retryWhenCloneInProgressError(),
            map(rev => !!rev),
            catchError(error => {
                if ((error as Error).name === ERPRIVATEREPOPUBLICSOURCEGRAPHCOM) {
                    return [false]
                }

                return [true]
            })
        )

    const openOptionsMenu = () => {
        sendMessage({
            type: 'openOptionsPage',
        })
    }

    injectViewContextOnSourcegraph(sourcegraphUrl, codeHost, ensureRepoExists, isInPage ? undefined : openOptionsMenu)

    // A stream of selections for the current code view. By default, selections
    // are parsed from the location hash, but the codeHost can provide an alternative implementation.
    const selectionsChanges: Observable<Selection[]> = codeHost.selectionsChanges
        ? codeHost.selectionsChanges()
        : fromEvent(window, 'hashchange').pipe(
              map(() => lprToSelectionsZeroIndexed(parseHash(window.location.hash))),
              distinctUntilChanged(isEqual),
              startWith([])
          )

    // Keeps track of all documents on the page since calling this function (should be once per page).
    let visibleViewComponents: ViewComponentData[] = []

    const codeViews = of(document.body).pipe(
        findCodeViews(codeHost),
        mergeMap(({ codeView, resolveFileInfo, ...rest }) =>
            resolveFileInfo(codeView).pipe(map(info => ({ info, codeView, ...rest })))
        ),
        switchMap(({ info, ...rest }) =>
            fetchFileContents(info).pipe(map(infoWithContents => ({ info: infoWithContents, ...rest })))
        ),
        observeOn(animationFrameScheduler)
    )

    subscriptions.add(
        combineLatest(codeViews, selectionsChanges).subscribe(
            ([{ codeView, info, dom, adjustPosition, getToolbarMount, toolbarButtonProps }, selections]) => {
                const originalDOM = dom
                dom = {
                    ...dom,
                    // If any parent element has the sourcegraph-extension-element
                    // class then that element does not have any code. We
                    // must check for "any parent element" because extensions
                    // create their DOM changes before the blob is tokenized
                    // into multiple elements.
                    getCodeElementFromTarget: (target: HTMLElement): HTMLElement | null =>
                        target.closest('.sourcegraph-extension-element') !== null
                            ? null
                            : originalDOM.getCodeElementFromTarget(target),
                }

                visibleViewComponents = [
                    // Either a normal file, or HEAD when codeView is a diff
                    {
                        type: 'textEditor',
                        item: {
                            uri: toURIWithPath(info),
                            languageId: getModeFromPath(info.filePath) || 'could not determine mode',
                            text: info.content!,
                        },
                        selections,
                        isActive: true,
                    },
                    // All the currently open documents, which are all now considered inactive.
                    ...visibleViewComponents.map(c => ({ ...c, isActive: false })),
                ]
                const roots: Model['roots'] = [{ uri: toRootURI(info), inputRevision: info.rev || '' }]

                // When codeView is a diff, add BASE too.
                if (info.baseContent && info.baseRepoName && info.baseCommitID && info.baseFilePath) {
                    visibleViewComponents.push({
                        type: 'textEditor',
                        item: {
                            uri: toURIWithPath({
                                repoName: info.baseRepoName,
                                commitID: info.baseCommitID,
                                filePath: info.baseFilePath,
                            }),
                            languageId: getModeFromPath(info.filePath) || 'could not determine mode',
                            text: info.baseContent,
                        },
                        // There is no notion of a selection on diff views yet, so this is empty.
                        selections: [],
                        isActive: false,
                    })
                    roots.push({
                        uri: toRootURI({
                            repoName: info.baseRepoName,
                            commitID: info.baseCommitID,
                        }),
                        inputRevision: info.baseRev || '',
                    })
                }

                let decoratedLines: number[] = []
                if (!info.baseCommitID) {
                    extensionsController.services.textDocumentDecoration
                        .getDecorations(toTextDocumentIdentifier(info))
                        .subscribe(decorations => {
                            decoratedLines = applyDecorations(dom, codeView, decorations || [], decoratedLines)
                        })
                }

                extensionsController.services.model.model.next({ roots, visibleViewComponents })

                const resolveContext: ContextResolver<RepoSpec & RevSpec & FileSpec & ResolvedRevSpec> = ({
                    part,
                }) => ({
                    repoName: part === 'base' ? info.baseRepoName || info.repoName : info.repoName,
                    commitID: part === 'base' ? info.baseCommitID! : info.commitID,
                    filePath: part === 'base' ? info.baseFilePath || info.filePath : info.filePath,
                    rev: part === 'base' ? info.baseRev || info.baseCommitID! : info.rev || info.commitID,
                })

                subscriptions.add(
                    hoverifier.hoverify({
                        dom,
                        positionEvents: of(codeView).pipe(findPositionsFromEvents(dom)),
                        resolveContext,
                        adjustPosition,
                    })
                )

                codeView.classList.add('sg-mounted')

                if (!getToolbarMount) {
                    return
                }

                const mount = getToolbarMount(codeView)

                render(
                    <TelemetryContext.Provider value={eventLogger}>
                        <CodeViewToolbar
                            {...info}
                            platformContext={platformContext}
                            extensionsController={extensionsController}
                            buttonProps={
                                toolbarButtonProps || {
                                    className: '',
                                    style: {},
                                }
                            }
                            location={H.createLocation(window.location)}
                        />
                    </TelemetryContext.Provider>,
                    mount
                )
            }
        )
    )

    return subscriptions
}

async function injectCodeIntelligenceToCodeHosts(codeHosts: CodeHost[]): Promise<Subscription> {
    const subscriptions = new Subscription()

    for (const codeHost of codeHosts) {
        const isCodeHost = await Promise.resolve(codeHost.check())
        if (isCodeHost) {
            subscriptions.add(handleCodeHost(codeHost))
            break
        }
    }

    return subscriptions
}

/**
 * Injects all code hosts into the page.
 *
 * @returns A promise with a subscription containing all subscriptions for code
 * intelligence. Unsubscribing will clean up subscriptions for hoverify and any
 * incomplete setup requests.
 */
export async function injectCodeIntelligence(): Promise<Subscription> {
    const codeHosts: CodeHost[] = [bitbucketServerCodeHost, githubCodeHost, gitlabCodeHost, phabricatorCodeHost]

    return await injectCodeIntelligenceToCodeHosts(codeHosts)
}
