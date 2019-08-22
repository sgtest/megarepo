import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import CheckIcon from 'mdi-react/CheckIcon'
import * as React from 'react'
import { ErrorLike, isErrorLike } from '../../../shared/src/util/errors'
import { CopyableText } from '../components/CopyableText'
import { Subscription, Subject, of } from 'rxjs'
import { catchError, tap, switchMap } from 'rxjs/operators'
import { backendRequest } from '../backend/backend'

interface Props {
    repoName: string
}

interface State {
    challengeOrError?: string | ErrorLike
    verifying?: boolean
    tokenOrError?: string | ErrorLike
}

interface ChallengeResponse {
    challenge: string
}

type VerifyResponse =
    | {
          failure: string
      }
    | { token: string }

export class LSIFVerification extends React.PureComponent<Props, State> {
    private verifies = new Subject<undefined>()
    private subscriptions = new Subscription()

    constructor(props: Props) {
        super(props)

        this.state = {}
    }

    public componentDidMount(): void {
        this.subscriptions.add(
            backendRequest<ChallengeResponse>('/.api/lsif/challenge').subscribe(
                ({ challenge }) => this.setState({ challengeOrError: challenge }),
                error =>
                    this.setState({
                        challengeOrError: new Error(
                            'Unable to fetch the LSIF challenge. Make sure lsifUploadSecret is set in the site configuration. Inner error: ' +
                                error.message
                        ),
                    })
            )
        )

        this.subscriptions.add(
            this.verifies
                .pipe(
                    tap(() => this.setState({ tokenOrError: undefined, verifying: true })),
                    switchMap(() => {
                        const url = new URL('/.api/lsif/verify', window.location.href)
                        url.searchParams.set('repository', this.props.repoName)
                        return backendRequest<VerifyResponse>(url.href).pipe(
                            tap(response => {
                                if ('failure' in response) {
                                    throw new Error(response.failure)
                                }
                                this.setState({ tokenOrError: response.token })
                            }),
                            catchError(error => {
                                this.setState({ tokenOrError: error })
                                return of(undefined)
                            })
                        )
                    }),
                    tap(() => this.setState({ verifying: false }))
                )
                .subscribe()
        )
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        if (isErrorLike(this.state.challengeOrError)) {
            return <div className="alert alert-danger">{this.state.challengeOrError.message}</div>
        }

        return (
            <div className="lsif-verification">
                {this.state.tokenOrError && !isErrorLike(this.state.tokenOrError) ? (
                    <div className="alert alert-success mb-1">
                        <CheckIcon className="icon-inline" /> Topic found. Here's the LSIF upload token:
                        <CopyableText text={this.state.tokenOrError} size={128} />
                        You can remove the topic now.
                    </div>
                ) : (
                    <>
                        <div className="mb-1">
                            To get an LSIF upload token for this repository, prove that you own the repository by adding
                            a GitHub topic to <a href={`https://${this.props.repoName}`}>{this.props.repoName}</a> with
                            the following name:
                        </div>

                        {this.state.challengeOrError ? (
                            <>
                                <CopyableText className="mb-1" text={this.state.challengeOrError} size={16} />
                                <button
                                    type="button"
                                    className="btn btn-primary mb-1"
                                    disabled={this.state.verifying}
                                    onClick={this.onClickVerify}
                                >
                                    {this.state.verifying && <LoadingSpinner className="icon-inline" />}
                                    Check now
                                </button>
                                {isErrorLike(this.state.tokenOrError) && (
                                    <div className="alert alert-danger mb-1">{this.state.tokenOrError.toString()}</div>
                                )}
                            </>
                        ) : (
                            <div className="mb-1">Loading challenge...</div>
                        )}
                    </>
                )}
            </div>
        )
    }

    private onClickVerify = () => this.verifies.next()
}
