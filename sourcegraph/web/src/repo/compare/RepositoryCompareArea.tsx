import { createHoverifier, HoveredToken, Hoverifier, HoverState } from '@sourcegraph/codeintellify'
import { isEqual, upperFirst } from 'lodash'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import MapSearchIcon from 'mdi-react/MapSearchIcon'
import * as React from 'react'
import { Route, RouteComponentProps, Switch } from 'react-router'
import { Subject, Subscription } from 'rxjs'
import { filter, map, withLatestFrom } from 'rxjs/operators'
import { ActionItemAction } from '../../../../shared/src/actions/ActionItem'
import { HoverMerged } from '../../../../shared/src/api/client/types/hover'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import * as GQL from '../../../../shared/src/graphql/schema'
import { getHoverActions } from '../../../../shared/src/hover/actions'
import { HoverContext } from '../../../../shared/src/hover/HoverOverlay'
import { getModeFromPath } from '../../../../shared/src/languages'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { propertyIsDefined } from '../../../../shared/src/util/types'
import {
    escapeRevspecForURL,
    FileSpec,
    ModeSpec,
    PositionSpec,
    RepoSpec,
    ResolvedRevSpec,
    RevSpec,
} from '../../../../shared/src/util/url'
import { getHover } from '../../backend/features'
import { HeroPage } from '../../components/HeroPage'
import { WebHoverOverlay } from '../../components/shared'
import { ThemeProps } from '../../theme'
import { EventLoggerProps } from '../../tracking/eventLogger'
import { RepoHeaderContributionsLifecycleProps } from '../RepoHeader'
import { RepoHeaderBreadcrumbNavItem } from '../RepoHeaderBreadcrumbNavItem'
import { RepoHeaderContributionPortal } from '../RepoHeaderContributionPortal'
import { RepositoryCompareHeader } from './RepositoryCompareHeader'
import { RepositoryCompareOverviewPage } from './RepositoryCompareOverviewPage'

const NotFoundPage = () => (
    <HeroPage
        icon={MapSearchIcon}
        title="404: Not Found"
        subtitle="Sorry, the requested repository comparison page was not found."
    />
)

interface RepositoryCompareAreaProps
    extends RouteComponentProps<{ spec: string }>,
        RepoHeaderContributionsLifecycleProps,
        PlatformContextProps,
        EventLoggerProps,
        ExtensionsControllerProps,
        ThemeProps {
    repo: GQL.IRepository
}

interface State extends HoverState<HoverContext, HoverMerged, ActionItemAction> {
    error?: string
}

/**
 * Properties passed to all page components in the repository compare area.
 */
export interface RepositoryCompareAreaPageProps extends PlatformContextProps {
    /** The repository being compared. */
    repo: GQL.IRepository

    /** The base of the comparison. */
    base: { repoName: string; repoID: GQL.ID; rev?: string | null }

    /** The head of the comparison. */
    head: { repoName: string; repoID: GQL.ID; rev?: string | null }

    /** The URL route prefix for the comparison. */
    routePrefix: string
}

/**
 * Renders pages related to a repository comparison.
 */
export class RepositoryCompareArea extends React.Component<RepositoryCompareAreaProps, State> {
    private componentUpdates = new Subject<RepositoryCompareAreaProps>()

    /** Emits whenever the ref callback for the hover element is called */
    private hoverOverlayElements = new Subject<HTMLElement | null>()
    private nextOverlayElement = (element: HTMLElement | null) => this.hoverOverlayElements.next(element)

    /** Emits whenever the ref callback for the hover element is called */
    private repositoryCompareAreaElements = new Subject<HTMLElement | null>()
    private nextRepositoryCompareAreaElement = (element: HTMLElement | null) =>
        this.repositoryCompareAreaElements.next(element)

    /** Emits when the close button was clicked */
    private closeButtonClicks = new Subject<MouseEvent>()
    private nextCloseButtonClick = (event: MouseEvent) => this.closeButtonClicks.next(event)

    private subscriptions = new Subscription()
    private hoverifier: Hoverifier<RepoSpec & RevSpec & FileSpec & ResolvedRevSpec, HoverMerged, ActionItemAction>

    constructor(props: RepositoryCompareAreaProps) {
        super(props)
        this.hoverifier = createHoverifier<
            RepoSpec & RevSpec & FileSpec & ResolvedRevSpec,
            HoverMerged,
            ActionItemAction
        >({
            closeButtonClicks: this.closeButtonClicks,
            hoverOverlayElements: this.hoverOverlayElements,
            hoverOverlayRerenders: this.componentUpdates.pipe(
                withLatestFrom(this.hoverOverlayElements, this.repositoryCompareAreaElements),
                map(([, hoverOverlayElement, repositoryCompareAreaElement]) => ({
                    hoverOverlayElement,
                    // The root component element is guaranteed to be rendered after a componentDidUpdate
                    relativeElement: repositoryCompareAreaElement!,
                })),
                // Can't reposition HoverOverlay if it wasn't rendered
                filter(propertyIsDefined('hoverOverlayElement'))
            ),
            getHover: hoveredToken => getHover(this.getLSPTextDocumentPositionParams(hoveredToken), this.props),
            getActions: context => getHoverActions(this.props, context),
            pinningEnabled: true,
        })
        this.subscriptions.add(this.hoverifier)
        this.state = this.hoverifier.hoverState
        this.subscriptions.add(this.hoverifier.hoverStateUpdates.subscribe(update => this.setState(update)))
    }

    private getLSPTextDocumentPositionParams(
        hoveredToken: HoveredToken & RepoSpec & RevSpec & FileSpec & ResolvedRevSpec
    ): RepoSpec & RevSpec & ResolvedRevSpec & FileSpec & PositionSpec & ModeSpec {
        return {
            repoName: hoveredToken.repoName,
            rev: hoveredToken.rev,
            filePath: hoveredToken.filePath,
            commitID: hoveredToken.commitID,
            position: hoveredToken,
            mode: getModeFromPath(hoveredToken.filePath || ''),
        }
    }

    public componentDidMount(): void {
        this.componentUpdates.next(this.props)
    }

    public shouldComponentUpdate(nextProps: Readonly<RepositoryCompareAreaProps>, nextState: Readonly<State>): boolean {
        return !isEqual(this.props, nextProps) || !isEqual(this.state, nextState)
    }

    public componentDidUpdate(): void {
        this.componentUpdates.next(this.props)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        if (this.state.error) {
            return <HeroPage icon={AlertCircleIcon} title="Error" subtitle={upperFirst(this.state.error)} />
        }

        let spec: { base: string | null; head: string | null } | null | undefined
        if (this.props.match.params.spec) {
            spec = parseComparisonSpec(this.props.match.params.spec)
        }

        const commonProps: RepositoryCompareAreaPageProps = {
            repo: this.props.repo,
            base: { repoID: this.props.repo.id, repoName: this.props.repo.name, rev: spec && spec.base },
            head: { repoID: this.props.repo.id, repoName: this.props.repo.name, rev: spec && spec.head },
            routePrefix: this.props.match.url,
            platformContext: this.props.platformContext,
        }

        return (
            <div className="repository-compare-area area--vertical" ref={this.nextRepositoryCompareAreaElement}>
                <RepoHeaderContributionPortal
                    position="nav"
                    element={<RepoHeaderBreadcrumbNavItem key="compare">Compare</RepoHeaderBreadcrumbNavItem>}
                    repoHeaderContributionsLifecycleProps={this.props.repoHeaderContributionsLifecycleProps}
                />
                <RepositoryCompareHeader
                    className="area--vertical__header"
                    {...commonProps}
                    onUpdateComparisonSpec={this.onUpdateComparisonSpec}
                />
                <div className="area--vertical__content">
                    <div className="area--vertical__content-inner">
                        {spec === null ? (
                            <div className="alert alert-danger">Invalid comparison specifier</div>
                        ) : (
                            <Switch>
                                <Route
                                    path={`${this.props.match.url}`}
                                    key="hardcoded-key" // see https://github.com/ReactTraining/react-router/issues/4578#issuecomment-334489490
                                    exact={true}
                                    // tslint:disable-next-line:jsx-no-lambda
                                    render={routeComponentProps => (
                                        <RepositoryCompareOverviewPage
                                            {...routeComponentProps}
                                            {...commonProps}
                                            hoverifier={this.hoverifier}
                                            isLightTheme={this.props.isLightTheme}
                                            extensionsController={this.props.extensionsController}
                                        />
                                    )}
                                />
                                <Route key="hardcoded-key" component={NotFoundPage} />
                            </Switch>
                        )}
                    </div>
                </div>
                {this.state.hoverOverlayProps && (
                    <WebHoverOverlay
                        {...this.props}
                        {...this.state.hoverOverlayProps}
                        telemetryService={this.props.telemetryService}
                        hoverRef={this.nextOverlayElement}
                        onCloseButtonClick={this.nextCloseButtonClick}
                    />
                )}
            </div>
        )
    }

    private onUpdateComparisonSpec = (newBaseSpec: string, newHeadSpec: string): void => {
        this.props.history.push(
            `/${this.props.repo.name}/-/compare${
                newBaseSpec || newHeadSpec
                    ? `/${escapeRevspecForURL(newBaseSpec || '')}...${escapeRevspecForURL(newHeadSpec || '')}`
                    : ''
            }`
        )
    }
}

function parseComparisonSpec(spec: string): { base: string | null; head: string | null } | null {
    if (!spec.includes('...')) {
        return null
    }
    const parts = spec.split('...', 2).map(decodeURIComponent)
    return {
        base: parts[0] || null,
        head: parts[1] || null,
    }
}
