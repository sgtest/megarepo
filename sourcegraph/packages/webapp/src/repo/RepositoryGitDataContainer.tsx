import { upperFirst } from 'lodash'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import * as React from 'react'
import { defer, Subject, Subscription } from 'rxjs'
import { catchError, delay, distinctUntilChanged, map, retryWhen, switchMap, tap } from 'rxjs/operators'
import { HeroPage } from '../components/HeroPage'
import { displayRepoPath } from '../components/RepoFileLink'
import { ErrorLike, isErrorLike } from '../util/errors'
import { RepoQuestionIcon } from '../util/icons'
import { RepositoryIcon } from '../util/icons' // TODO: Switch to mdi icon
import { CloneInProgressError, ECLONEINPROGESS, EREVNOTFOUND, resolveRev } from './backend'
import { DirectImportRepoAlert } from './DirectImportRepoAlert'

export const RepositoryCloningInProgressPage: React.SFC<{ repoName: string; progress?: string }> = ({
    repoName,
    progress,
}) => (
    <HeroPage
        icon={RepositoryIcon}
        title={displayRepoPath(repoName)}
        className="repository-cloning-in-progress-page"
        subtitle="Cloning in progress"
        detail={<code>{progress}</code>}
        body={<DirectImportRepoAlert className="mt-3" />}
    />
)

export const EmptyRepositoryPage: React.SFC = () => <HeroPage icon={RepoQuestionIcon} title="Empty repository" />

interface Props {
    /** The repository. */
    repoPath: string

    /** The fragment to render if the repository's Git data is accessible. */
    children: React.ReactNode
}

interface State {
    /**
     * True if the repository's Git data is cloned and non-empty, undefined while loading, or an error (including
     * if cloning is in progress).
     */
    gitDataPresentOrError?: true | ErrorLike
}

/**
 * A container for a repository page that incorporates global Git data but is not tied to one specific revision. A
 * loading/error page is shown if the repository is not yet cloned or is empty. Otherwise, the children are
 * rendered.
 */
export class RepositoryGitDataContainer extends React.PureComponent<Props, State> {
    public state: State = {}

    private propsUpdates = new Subject<Props>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        // Fetch repository revision.
        this.subscriptions.add(
            this.propsUpdates
                .pipe(
                    map(({ repoPath }) => repoPath),
                    distinctUntilChanged(),
                    tap(() => this.setState({ gitDataPresentOrError: undefined })),
                    switchMap(repoPath =>
                        defer(() => resolveRev({ repoPath })).pipe(
                            // On a CloneInProgress error, retry after 1s
                            retryWhen(errors =>
                                errors.pipe(
                                    tap(error => {
                                        switch (error.code) {
                                            case ECLONEINPROGESS:
                                                // Display cloning screen to the user and retry
                                                this.setState({ gitDataPresentOrError: error })
                                                return
                                            default:
                                                // Display error to the user and do not retry
                                                throw error
                                        }
                                    }),
                                    delay(1000)
                                )
                            ),
                            // Save any error in the state to display to the user
                            catchError(error => {
                                this.setState({ gitDataPresentOrError: error })
                                return []
                            })
                        )
                    )
                )
                .subscribe(resolvedRev => this.setState({ gitDataPresentOrError: true }), error => console.error(error))
        )
        this.propsUpdates.next(this.props)
    }

    public componentWillReceiveProps(props: Props): void {
        this.propsUpdates.next(props)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): React.ReactNode | React.ReactNode[] | null {
        if (!this.state.gitDataPresentOrError) {
            // Render nothing while loading
            return null
        }

        if (isErrorLike(this.state.gitDataPresentOrError)) {
            // Show error page
            switch (this.state.gitDataPresentOrError.code) {
                case ECLONEINPROGESS:
                    return (
                        <RepositoryCloningInProgressPage
                            repoName={this.props.repoPath}
                            progress={(this.state.gitDataPresentOrError as CloneInProgressError).progress}
                        />
                    )
                case EREVNOTFOUND:
                    return <EmptyRepositoryPage />
                default:
                    return (
                        <HeroPage
                            icon={AlertCircleIcon}
                            title="Error"
                            subtitle={upperFirst(this.state.gitDataPresentOrError.message)}
                        />
                    )
            }
        }

        return this.props.children
    }
}
