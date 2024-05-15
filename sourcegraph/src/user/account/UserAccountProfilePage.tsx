import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { upperFirst } from 'lodash'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Link } from 'react-router-dom'
import { combineLatest, Observable, Subject, Subscription } from 'rxjs'
import {
    catchError,
    concat,
    distinctUntilChanged,
    filter,
    map,
    mergeMap,
    startWith,
    switchMap,
    tap,
} from 'rxjs/operators'
import { USER_DISPLAY_NAME_MAX_LENGTH, userURL } from '..'
import { refreshCurrentUser } from '../../auth'
import { UsernameInput } from '../../auth/SignInSignUpCommon'
import { gql, queryGraphQL } from '../../backend/graphql'
import * as GQL from '../../backend/graphqlschema'
import { Form } from '../../components/Form'
import { PageTitle } from '../../components/PageTitle'
import { eventLogger } from '../../tracking/eventLogger'
import { asError, createAggregateError, ErrorLike, isErrorLike } from '../../util/errors'
import { enableUserArea, UserAreaPageProps } from '../area/UserArea'
import { UserAvatar } from '../UserAvatar'
import { updateUser } from './backend'

function queryUser(user: GQL.ID): Observable<GQL.IUser> {
    return queryGraphQL(
        gql`
            query User($user: ID!) {
                node(id: $user) {
                    ... on User {
                        id
                        username
                        displayName
                        avatarURL
                    }
                }
            }
        `,
        { user }
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.node) {
                throw createAggregateError(errors)
            }
            return data.node as GQL.IUser
        })
    )
}

interface Props extends UserAreaPageProps, RouteComponentProps<{}> {}

interface State {
    /** The user to edit, or an error, or undefined while loading. */
    userOrError?: GQL.IUser | ErrorLike

    loading: boolean
    saved: boolean
    error?: ErrorLike

    /** undefined means unchanged from Props.user */
    username?: string
    displayName?: string
    avatarURL?: string
}

export class UserAccountProfilePage extends React.Component<Props, State> {
    public state: State = { loading: false, saved: false }

    private componentUpdates = new Subject<Props>()
    private refreshRequests = new Subject<void>()
    private submits = new Subject<React.FormEvent<HTMLFormElement>>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        eventLogger.logViewEvent('UserProfile')

        const userChanges = this.componentUpdates.pipe(
            distinctUntilChanged((a, b) => a.user.id === b.user.id),
            map(({ user }) => user)
        )

        // Reset the fields upon navigation to a different user.
        this.subscriptions.add(
            userChanges.subscribe(() =>
                this.setState({
                    userOrError: undefined,
                    loading: false,
                    saved: false,
                    username: undefined,
                    displayName: undefined,
                    avatarURL: undefined,
                })
            )
        )

        // Fetch the user with all of the fields we need (Props.user might not have them all).
        this.subscriptions.add(
            combineLatest(userChanges, this.refreshRequests.pipe(startWith<void>(void 0)))
                .pipe(
                    switchMap(([user]) =>
                        queryUser(user.id).pipe(
                            catchError(error => [asError(error)]),
                            map(c => ({ userOrError: c } as Pick<State, 'userOrError'>))
                        )
                    )
                )
                .subscribe(stateUpdate => this.setState(stateUpdate), err => console.error(err))
        )

        this.subscriptions.add(
            this.submits
                .pipe(
                    tap(event => {
                        event.preventDefault()
                        eventLogger.log('UpdateUserClicked')
                    }),
                    filter(event => event.currentTarget.checkValidity()),
                    tap(() => this.setState({ loading: true })),
                    mergeMap(event =>
                        updateUser(this.props.user.id, {
                            username: this.state.username === undefined ? null : this.state.username,
                            displayName: this.state.displayName === undefined ? null : this.state.displayName,
                            avatarURL: this.state.avatarURL === undefined ? null : this.state.avatarURL,
                        }).pipe(catchError(this.handleError))
                    ),
                    tap(() => {
                        this.setState({ loading: false, saved: true })
                        this.props.onDidUpdateUser()

                        // Handle when username changes.
                        if (this.state.username !== undefined && this.state.username !== this.props.user.username) {
                            this.props.history.push(`/users/${this.state.username}/settings/profile`)
                            return
                        }

                        this.refreshRequests.next()
                        setTimeout(() => this.setState({ saved: false }), 500)
                    }),

                    // In case the edited user is the current user, immediately reflect the changes in the UI.
                    mergeMap(() => refreshCurrentUser().pipe(concat([null])))
                )
                .subscribe(undefined, this.handleError)
        )

        this.componentUpdates.next(this.props)
    }

    public componentWillReceiveProps(nextProps: Props): void {
        this.componentUpdates.next(nextProps)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        return (
            <div className="user-settings-profile-page">
                <PageTitle title="Profile" />
                <h2>Profile</h2>
                {isErrorLike(this.state.userOrError) && (
                    <p className="alert alert-danger">Error: {upperFirst(this.state.userOrError.message)}</p>
                )}
                {this.state.error && (
                    <p className="alert alert-danger">Error: {upperFirst(this.state.error.message)}</p>
                )}
                {this.state.userOrError &&
                    !isErrorLike(this.state.userOrError) && (
                        <Form className="user-settings-profile-page__form" onSubmit={this.handleSubmit}>
                            <div className="form-group">
                                <label htmlFor="user-settings-profile-page__form-username">Username</label>
                                <UsernameInput
                                    id="user-settings-profile-page__form-username"
                                    value={
                                        this.state.username === undefined
                                            ? this.state.userOrError.username
                                            : this.state.username
                                    }
                                    onChange={this.onUsernameFieldChange}
                                    required={true}
                                    disabled={this.state.loading}
                                    aria-describedby="user-settings-profile-page__form-username-help"
                                />
                                <small
                                    id="user-settings-profile-page__form-username-help"
                                    className="form-text text-muted"
                                >
                                    A username consists of letters, numbers, hyphens (-) and may not begin or end with a
                                    hyphen
                                </small>
                            </div>
                            <div className="form-group">
                                <label htmlFor="user-settings-profile-page__form-display-name">Display name</label>
                                <input
                                    id="user-settings-profile-page__form-display-name"
                                    type="text"
                                    className="form-control"
                                    value={
                                        this.state.displayName === undefined
                                            ? this.state.userOrError.displayName || ''
                                            : this.state.displayName
                                    }
                                    onChange={this.onDisplayNameFieldChange}
                                    disabled={this.state.loading}
                                    spellCheck={false}
                                    placeholder="Display name"
                                    maxLength={USER_DISPLAY_NAME_MAX_LENGTH}
                                />
                            </div>
                            <div className="user-settings-profile-page__avatar-row">
                                <div className="form-group user-settings-profile-page__field-column">
                                    <label htmlFor="user-settings-profile-page__form-avatar-url">Avatar URL</label>
                                    <input
                                        id="user-settings-profile-page__form-avatar-url"
                                        type="url"
                                        className="form-control"
                                        value={
                                            this.state.avatarURL === undefined
                                                ? this.state.userOrError.avatarURL || ''
                                                : this.state.avatarURL
                                        }
                                        onChange={this.onAvatarURLFieldChange}
                                        disabled={this.state.loading}
                                        spellCheck={false}
                                        placeholder="URL to avatar photo"
                                    />
                                </div>
                                {this.state.userOrError.avatarURL && (
                                    <div className="user-settings-profile-page__avatar-column">
                                        <UserAvatar
                                            user={
                                                this.state.userOrError.avatarURL
                                                    ? { avatarURL: this.state.userOrError.avatarURL }
                                                    : undefined
                                            }
                                        />
                                    </div>
                                )}
                            </div>
                            <button
                                className="btn btn-primary user-settings-profile-page__button"
                                type="submit"
                                disabled={this.state.loading}
                            >
                                Update profile
                            </button>
                            {this.state.loading && (
                                <div>
                                    <LoadingSpinner className="icon-inline" />
                                </div>
                            )}
                            {this.state.saved && (
                                <p className="alert alert-success user-settings-profile-page__alert">Profile saved!</p>
                            )}
                            {enableUserArea &&
                                !this.state.saved &&
                                !this.state.loading && (
                                    <Link to={userURL(this.props.user.username)} className="btn btn-link">
                                        View public profile
                                    </Link>
                                )}
                            {window.context.sourcegraphDotComMode && (
                                <p className="mt-4">
                                    <a href="https://about.sourcegraph.com/contact">Contact support</a> to delete your
                                    account.
                                </p>
                            )}
                        </Form>
                    )}
            </div>
        )
    }

    private onUsernameFieldChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        this.setState({ username: e.target.value })
    }

    private onDisplayNameFieldChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        this.setState({ displayName: e.target.value })
    }

    private onAvatarURLFieldChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        this.setState({ avatarURL: e.target.value })
    }

    private handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
        this.submits.next(event)
    }

    private handleError = (err: Error) => {
        console.error(err)
        this.setState({ loading: false, saved: false, error: err })
        return []
    }
}
