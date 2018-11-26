import {
    createHoverifier,
    HoveredToken,
    HoveredTokenContext,
    Hoverifier,
    HoverOverlay,
    HoverState,
} from '@sourcegraph/codeintellify'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { isEqual, upperFirst } from 'lodash'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Link, LinkProps } from 'react-router-dom'
import { merge, Observable, of, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, filter, map, switchMap, tap, withLatestFrom } from 'rxjs/operators'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import { gql } from '../../../../shared/src/graphql/graphql'
import * as GQL from '../../../../shared/src/graphql/schema'
import { getModeFromPath } from '../../../../shared/src/languages'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { asError, createAggregateError, ErrorLike, isErrorLike } from '../../../../shared/src/util/errors'
import { getHover, getJumpURL } from '../../backend/features'
import { queryGraphQL } from '../../backend/graphql'
import { LSPTextDocumentPositionParams } from '../../backend/lsp'
import { PageTitle } from '../../components/PageTitle'
import { ExtensionsDocumentsProps } from '../../extensions/environment/ExtensionsEnvironment'
import { eventLogger } from '../../tracking/eventLogger'
import { memoizeObservable } from '../../util/memoize'
import { propertyIsDefined } from '../../util/types'
import { GitCommitNode } from '../commits/GitCommitNode'
import { gitCommitFragment } from '../commits/RepositoryCommitsPage'
import { FileDiffConnection } from '../compare/FileDiffConnection'
import { FileDiffNode } from '../compare/FileDiffNode'
import { queryRepositoryComparisonFileDiffs } from '../compare/RepositoryCompareDiffPage'

const queryCommit = memoizeObservable(
    (args: { repo: GQL.ID; revspec: string }): Observable<GQL.IGitCommit> =>
        queryGraphQL(
            gql`
                query RepositoryCommit($repo: ID!, $revspec: String!) {
                    node(id: $repo) {
                        ... on Repository {
                            commit(rev: $revspec) {
                                __typename # necessary so that isErrorLike(x) is false when x: GQL.IGitCommit
                                ...GitCommitFields
                            }
                        }
                    }
                }
                ${gitCommitFragment}
            `,
            args
        ).pipe(
            map(({ data, errors }) => {
                if (!data || !data.node) {
                    throw createAggregateError(errors)
                }
                const repo = data.node as GQL.IRepository
                if (!repo.commit) {
                    throw createAggregateError(errors || [new Error('Commit not found')])
                }
                return repo.commit
            })
        ),
    args => `${args.repo}:${args.revspec}`
)

interface Props
    extends RouteComponentProps<{ revspec: string }>,
        PlatformContextProps,
        ExtensionsControllerProps,
        ExtensionsDocumentsProps {
    repo: GQL.IRepository

    onDidUpdateExternalLinks: (externalLinks: GQL.IExternalLink[] | undefined) => void
}

interface State extends HoverState {
    /** The commit, undefined while loading, or an error. */
    commitOrError?: GQL.IGitCommit | ErrorLike
}

const logTelemetryEvent = (event: string, data?: any) => eventLogger.log(event, data)
const LinkComponent = (props: LinkProps) => <Link {...props} />

/** Displays a commit. */
export class RepositoryCommitPage extends React.Component<Props, State> {
    private componentUpdates = new Subject<Props>()

    /** Emits whenever the ref callback for the hover element is called */
    private hoverOverlayElements = new Subject<HTMLElement | null>()
    private nextOverlayElement = (element: HTMLElement | null) => this.hoverOverlayElements.next(element)

    /** Emits whenever the ref callback for the hover element is called */
    private repositoryCommitPageElements = new Subject<HTMLElement | null>()
    private nextRepositoryCommitPageElement = (element: HTMLElement | null) =>
        this.repositoryCommitPageElements.next(element)

    /** Emits when the go to definition button was clicked */
    private goToDefinitionClicks = new Subject<MouseEvent>()
    private nextGoToDefinitionClick = (event: MouseEvent) => this.goToDefinitionClicks.next(event)

    /** Emits when the close button was clicked */
    private closeButtonClicks = new Subject<MouseEvent>()
    private nextCloseButtonClick = (event: MouseEvent) => this.closeButtonClicks.next(event)

    private subscriptions = new Subscription()
    private hoverifier: Hoverifier

    constructor(props: Props) {
        super(props)
        this.hoverifier = createHoverifier({
            closeButtonClicks: this.closeButtonClicks,
            goToDefinitionClicks: this.goToDefinitionClicks,
            hoverOverlayElements: this.hoverOverlayElements,
            hoverOverlayRerenders: this.componentUpdates.pipe(
                withLatestFrom(this.hoverOverlayElements, this.repositoryCommitPageElements),
                map(([, hoverOverlayElement, repositoryCommitPageElement]) => ({
                    hoverOverlayElement,
                    // The root component element is guaranteed to be rendered after a componentDidUpdate
                    relativeElement: repositoryCommitPageElement!,
                })),
                // Can't reposition HoverOverlay if it wasn't rendered
                filter(propertyIsDefined('hoverOverlayElement'))
            ),
            pushHistory: path => this.props.history.push(path),
            logTelemetryEvent,
            fetchHover: hoveredToken => getHover(this.getLSPTextDocumentPositionParams(hoveredToken), this.props),
            fetchJumpURL: hoveredToken => getJumpURL(this.getLSPTextDocumentPositionParams(hoveredToken), this.props),
        })
        this.subscriptions.add(this.hoverifier)
        this.state = this.hoverifier.hoverState
        this.subscriptions.add(
            this.hoverifier.hoverStateUpdates.subscribe(update => {
                this.setState(update)
            })
        )
    }

    private getLSPTextDocumentPositionParams(
        hoveredToken: HoveredToken & HoveredTokenContext
    ): LSPTextDocumentPositionParams {
        return {
            repoPath: hoveredToken.repoPath,
            rev: hoveredToken.rev,
            filePath: hoveredToken.filePath,
            commitID: hoveredToken.commitID,
            position: hoveredToken,
            mode: getModeFromPath(hoveredToken.filePath || ''),
        }
    }

    public componentDidMount(): void {
        eventLogger.logViewEvent('RepositoryCommit')

        this.subscriptions.add(
            this.componentUpdates
                .pipe(
                    distinctUntilChanged(
                        (a, b) => a.repo.id === b.repo.id && a.match.params.revspec === b.match.params.revspec
                    ),
                    switchMap(({ repo, match }) =>
                        merge(
                            of({ commitOrError: undefined }),
                            queryCommit({ repo: repo.id, revspec: match.params.revspec }).pipe(
                                catchError(error => [asError(error)]),
                                map(c => ({ commitOrError: c })),
                                tap(({ commitOrError }: { commitOrError: GQL.IGitCommit | ErrorLike }) => {
                                    if (isErrorLike(commitOrError)) {
                                        this.props.onDidUpdateExternalLinks(undefined)
                                    } else {
                                        this.props.onDidUpdateExternalLinks(commitOrError.externalURLs)
                                    }
                                })
                            )
                        )
                    )
                )
                .subscribe(stateUpdate => this.setState(stateUpdate), error => console.error(error))
        )
        this.componentUpdates.next(this.props)
    }

    public shouldComponentUpdate(nextProps: Readonly<Props>, nextState: Readonly<State>): boolean {
        return !isEqual(this.props, nextProps) || !isEqual(this.state, nextState)
    }

    public componentDidUpdate(): void {
        this.componentUpdates.next(this.props)
    }

    public componentWillUnmount(): void {
        this.props.onDidUpdateExternalLinks(undefined)
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        return (
            <div className="repository-commit-page area" ref={this.nextRepositoryCommitPageElement}>
                <PageTitle
                    title={
                        this.state.commitOrError && !isErrorLike(this.state.commitOrError)
                            ? this.state.commitOrError.subject
                            : `Commit ${this.props.match.params.revspec}`
                    }
                />
                <div className="area__content">
                    {this.state.commitOrError === undefined ? (
                        <LoadingSpinner className="icon-inline mt-2" />
                    ) : isErrorLike(this.state.commitOrError) ? (
                        <div className="alert alert-danger mt-2">
                            Error: {upperFirst(this.state.commitOrError.message)}
                        </div>
                    ) : (
                        <>
                            <div className="card repository-commit-page__card">
                                <div className="card-body">
                                    <GitCommitNode
                                        node={this.state.commitOrError}
                                        repoName={this.props.repo.name}
                                        expandCommitMessageBody={true}
                                        showSHAAndParentsRow={true}
                                    />
                                </div>
                            </div>
                            <div className="mb-3" />
                            <FileDiffConnection
                                listClassName="list-group list-group-flush"
                                noun="changed file"
                                pluralNoun="changed files"
                                queryConnection={this.queryDiffs}
                                nodeComponent={FileDiffNode}
                                nodeComponentProps={{
                                    base: {
                                        repoPath: this.props.repo.name,
                                        repoID: this.props.repo.id,
                                        rev: commitParentOrEmpty(this.state.commitOrError),
                                        commitID: commitParentOrEmpty(this.state.commitOrError),
                                    },
                                    head: {
                                        repoPath: this.props.repo.name,
                                        repoID: this.props.repo.id,
                                        rev: this.state.commitOrError.oid,
                                        commitID: this.state.commitOrError.oid,
                                    },
                                    lineNumbers: true,
                                    platformContext: this.props.platformContext,
                                    location: this.props.location,
                                    history: this.props.history,
                                    hoverifier: this.hoverifier,
                                    extensionsController: this.props.extensionsController,
                                }}
                                updateOnChange={`${this.props.repo.id}:${this.state.commitOrError.oid}`}
                                defaultFirst={25}
                                hideSearch={true}
                                noSummaryIfAllNodesVisible={true}
                                history={this.props.history}
                                location={this.props.location}
                                extensionsOnVisibleTextDocumentsChange={
                                    this.props.extensionsOnVisibleTextDocumentsChange
                                }
                                extensionsOnRootsChange={this.props.extensionsOnRootsChange}
                            />
                        </>
                    )}
                </div>
                {this.state.hoverOverlayProps && (
                    <HoverOverlay
                        {...this.state.hoverOverlayProps}
                        logTelemetryEvent={logTelemetryEvent}
                        linkComponent={LinkComponent}
                        hoverRef={this.nextOverlayElement}
                        onGoToDefinitionClick={this.nextGoToDefinitionClick}
                        onCloseButtonClick={this.nextCloseButtonClick}
                    />
                )}
            </div>
        )
    }

    private queryDiffs = (args: { first?: number }): Observable<GQL.IFileDiffConnection> =>
        queryRepositoryComparisonFileDiffs({
            ...args,
            repo: this.props.repo.id,
            base: commitParentOrEmpty(this.state.commitOrError as GQL.IGitCommit),
            head: (this.state.commitOrError as GQL.IGitCommit).oid,
        })
}

function commitParentOrEmpty(commit: GQL.IGitCommit): string {
    // 4b825dc642cb6eb9a060e54bf8d69288fbee4904 is `git hash-object -t tree /dev/null`, which is used as the base
    // when computing the `git diff` of the root commit.
    return commit.parents.length > 0 ? commit.parents[0].oid : '4b825dc642cb6eb9a060e54bf8d69288fbee4904'
}
