import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as H from 'history'
import { isEqual } from 'lodash'
import * as React from 'react'
import { Redirect } from 'react-router'
import { combineLatest, Subject, Subscription, throwError, Observable } from 'rxjs'
import { catchError, delay, distinctUntilChanged, map, repeatWhen, startWith, switchMap, tap } from 'rxjs/operators'
import { ExtensionsControllerProps } from '../../../../../shared/src/extensions/controller'
import * as GQL from '../../../../../shared/src/graphql/schema'
import { asError } from '../../../../../shared/src/util/errors'
import { addCommentToThread, fetchDiscussionThreadAndComments, updateComment } from '../../../discussions/backend'
import { DiscussionsComment } from '../../../discussions/DiscussionsComment'
import { eventLogger } from '../../../tracking/eventLogger'
import { formatHash } from '../../../util/url'
import { DiscussionsInput, TitleMode } from './DiscussionsInput'
import { DiscussionsNavbar } from './DiscussionsNavbar'
import { ErrorAlert } from '../../../components/alerts'

interface Props extends ExtensionsControllerProps {
    threadIDWithoutKind: string
    commentIDWithoutKind?: string
    repoID: GQL.ID
    rev: string | undefined
    filePath: string
    history: H.History
    location: H.Location
}

interface State {
    loading: boolean
    error?: any
    thread?: GQL.IDiscussionThread
}

export class DiscussionsThread extends React.PureComponent<Props, State> {
    private componentUpdates = new Subject<Props>()
    private subscriptions = new Subscription()

    constructor(props: Props) {
        super(props)
        this.state = {
            loading: true,
        }
    }

    public componentDidMount(): void {
        eventLogger.logViewEvent('DiscussionsThread')

        // TODO(slimsag:discussions): ASAP: changing threadID manually in URL does not work. Can't click links to threads/comments effectively.
        this.subscriptions.add(
            combineLatest(this.componentUpdates.pipe(startWith(this.props)))
                .pipe(
                    distinctUntilChanged(([a], [b]) => a.threadIDWithoutKind === b.threadIDWithoutKind),
                    switchMap(([props]) =>
                        fetchDiscussionThreadAndComments(props.threadIDWithoutKind).pipe(
                            map(thread => ({ thread, error: undefined, loading: false })),
                            catchError(error => {
                                console.error(error)
                                return [{ error, loading: false }]
                            }),
                            repeatWhen(delay(2500))
                        )
                    )
                )
                .subscribe(
                    stateUpdate => this.setState(state => ({ ...state, ...stateUpdate })),
                    err => console.error(err)
                )
        )
        this.componentUpdates.next(this.props)
    }

    public componentDidUpdate(): void {
        this.componentUpdates.next(this.props)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        // TODO(slimsag:discussions): future: test error state + cleanup CSS

        const { error, loading, thread } = this.state
        const { location, commentIDWithoutKind } = this.props

        // If the thread is loaded, ensure that the URL hash is updated to
        // reflect the line that the discussion was created on.
        if (thread) {
            const desiredHash = this.urlHashWithLine(
                thread,
                commentIDWithoutKind ? { idWithoutKind: commentIDWithoutKind } : undefined
            )
            if (!hashesEqual(desiredHash, location.hash)) {
                const discussionURL = location.pathname + location.search + desiredHash
                return <Redirect to={discussionURL} />
            }
        }

        return (
            <div className="discussions-thread">
                <DiscussionsNavbar {...this.props} threadTitle={thread ? thread.title : undefined} />
                {loading && <LoadingSpinner className="icon-inline" />}
                {error && (
                    <ErrorAlert
                        className="discussions-thread__error"
                        prefix="Error loading thread"
                        error={error}
                        history={this.props.history}
                    />
                )}
                {thread && (
                    <div className="discussions-thread__comments">
                        {thread.comments.nodes.map(node => (
                            <DiscussionsComment
                                key={node.id}
                                {...this.props}
                                threadID={thread.id}
                                comment={node}
                                onReport={this.onCommentReport}
                                onClearReports={this.onCommentClearReports}
                                onDelete={this.onCommentDelete}
                                extensionsController={this.props.extensionsController}
                            />
                        ))}
                        <DiscussionsInput
                            key="input"
                            submitLabel="Comment"
                            titleMode={TitleMode.None}
                            onSubmit={this.onSubmit}
                            {...this.props}
                        />
                    </div>
                )}
            </div>
        )
    }

    /**
     * Produces a URL hash for linking to the given discussion thread and the
     * line that it was created on.
     *
     * @param thread The thread to link to.
     */
    private urlHashWithLine(
        thread: Pick<GQL.IDiscussionThread, 'idWithoutKind' | 'target'>,
        comment?: Pick<GQL.IDiscussionComment, 'idWithoutKind'>
    ): string {
        const hash = new URLSearchParams()
        hash.set('tab', 'discussions')
        hash.set('threadID', thread.idWithoutKind)
        if (comment) {
            hash.set('commentID', comment.idWithoutKind)
        }

        return thread.target.__typename === 'DiscussionThreadTargetRepo' && thread.target.selection !== null
            ? formatHash(
                  {
                      line: thread.target.selection.startLine + 1,
                      character: thread.target.selection.startCharacter,
                      endLine:
                          // The 0th character means the selection ended at the end of the previous
                          // line.
                          (thread.target.selection.endCharacter === 0
                              ? thread.target.selection.endLine - 1
                              : thread.target.selection.endLine) + 1,
                      endCharacter: thread.target.selection.endCharacter,
                  },
                  hash
              )
            : '#' + hash.toString()
    }

    private onSubmit = (title: string, contents: string): Observable<void> => {
        eventLogger.log('RepliedToDiscussion')
        if (!this.state.thread) {
            throw new Error('no thread')
        }
        return addCommentToThread(this.state.thread.id, contents).pipe(
            tap(thread => this.setState({ thread })),
            map(() => undefined),
            catchError(e => throwError(new Error('Error creating comment: ' + asError(e).message)))
        )
    }

    private onCommentReport = (comment: GQL.IDiscussionComment, reason: string): Observable<void> =>
        updateComment({ commentID: comment.id, report: reason }).pipe(
            tap(thread => this.setState({ thread })),
            map(() => undefined)
        )

    private onCommentClearReports = (comment: GQL.IDiscussionComment): Observable<void> =>
        updateComment({ commentID: comment.id, clearReports: true }).pipe(
            tap(thread => this.setState({ thread })),
            map(() => undefined)
        )

    private onCommentDelete = (comment: GQL.IDiscussionComment): Observable<void> =>
        // TODO: Support deleting the whole thread, and/or fix this when it is deleting the 1st comment
        // in a thread. See https://github.com/sourcegraph/sourcegraph/issues/429.
        updateComment({ commentID: comment.id, delete: true }).pipe(
            tap(thread => this.setState({ thread })),
            map(thread => undefined)
        )
}

/**
 * @returns Whether the 2 URI fragments contain the same keys and values (assuming they contain a
 * `#` then HTML-form-encoded keys and values like `a=b&c=d`).
 */
function hashesEqual(a: string, b: string): boolean {
    if (a.startsWith('#')) {
        a = a.slice(1)
    }
    if (b.startsWith('#')) {
        b = b.slice(1)
    }
    const canonicalize = (hash: string): string[] =>
        Array.from(new URLSearchParams(hash).entries())
            .map(([key, value]) => `${key}=${value}`)
            .sort()
    return isEqual(canonicalize(a), canonicalize(b))
}
