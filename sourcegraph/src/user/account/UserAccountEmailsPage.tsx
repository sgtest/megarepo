import DeleteIcon from 'mdi-react/DeleteIcon'
import * as React from 'react'
import { RouteComponentProps } from 'react-router'
import { Observable, Subject, Subscription } from 'rxjs'
import { map } from 'rxjs/operators'
import { gql, mutateGraphQL, queryGraphQL } from '../../backend/graphql'
import * as GQL from '../../backend/graphqlschema'
import { FilteredConnection } from '../../components/FilteredConnection'
import { PageTitle } from '../../components/PageTitle'
import { SiteFlags } from '../../site'
import { siteFlags } from '../../site/backend'
import { eventLogger } from '../../tracking/eventLogger'
import { createAggregateError } from '../../util/errors'
import { AddUserEmailForm } from './AddUserEmailForm'
import { setUserEmailVerified } from './backend'

interface UserEmailNodeProps {
    node: GQL.IUserEmail
    user: GQL.IUser

    onDidUpdate: () => void
}

interface UserEmailNodeState {
    loading: boolean
    errorDescription?: string
}

class UserEmailNode extends React.PureComponent<UserEmailNodeProps, UserEmailNodeState> {
    public state: UserEmailNodeState = {
        loading: false,
    }

    public render(): JSX.Element | null {
        let statusFragment: React.ReactFragment
        if (this.props.node.verified) {
            statusFragment = <span className="badge badge-success">Verified</span>
        } else if (this.props.node.verificationPending) {
            statusFragment = <span className="badge badge-info">Verification pending</span>
        } else {
            statusFragment = <span className="badge badge-secondary">Not verified</span>
        }

        return (
            <li className="list-group-item py-2">
                <div className="d-flex align-items-center justify-content-between">
                    <div>
                        <strong>{this.props.node.email}</strong> &nbsp;{statusFragment}
                    </div>
                    <div>
                        <button
                            className="btn btn-sm btn-danger"
                            onClick={this.remove}
                            disabled={this.state.loading}
                            data-tooltip="Remove email address"
                        >
                            <DeleteIcon className="icon-inline" />
                        </button>{' '}
                        {this.props.node.viewerCanManuallyVerify && (
                            <button
                                className="btn btn-sm btn-secondary"
                                onClick={this.props.node.verified ? this.setAsUnverified : this.setAsVerified}
                                disabled={this.state.loading}
                            >
                                {this.props.node.verified ? 'Mark as unverified' : 'Mark as verified'}
                            </button>
                        )}
                    </div>
                </div>
                {this.state.errorDescription && (
                    <div className="alert alert-danger mt-2">{this.state.errorDescription}</div>
                )}
            </li>
        )
    }

    private remove = () => {
        if (!window.confirm(`Really remove the email address ${this.props.node.email}?`)) {
            return
        }

        this.setState({
            errorDescription: undefined,
            loading: true,
        })

        mutateGraphQL(
            gql`
                mutation RemoveUserEmail($user: ID!, $email: String!) {
                    removeUserEmail(user: $user, email: $email) {
                        alwaysNil
                    }
                }
            `,
            { user: this.props.user.id, email: this.props.node.email }
        )
            .pipe(
                map(({ data, errors }) => {
                    if (!data || (errors && errors.length > 0)) {
                        throw createAggregateError(errors)
                    }
                })
            )
            .subscribe(
                () => {
                    this.setState({ loading: false })
                    eventLogger.log('UserEmailAddressDeleted')
                    if (this.props.onDidUpdate) {
                        this.props.onDidUpdate()
                    }
                },
                error => this.setState({ loading: false, errorDescription: error.message })
            )
    }

    private setAsVerified = () => this.setVerified(true)
    private setAsUnverified = () => this.setVerified(false)

    private setVerified(verified: boolean): void {
        this.setState({
            errorDescription: undefined,
            loading: true,
        })

        setUserEmailVerified(this.props.user.id, this.props.node.email, verified).subscribe(
            () => {
                this.setState({ loading: false })
                if (verified) {
                    eventLogger.log('UserEmailAddressMarkedVerified')
                } else {
                    eventLogger.log('UserEmailAddressMarkedUnverified')
                }
                if (this.props.onDidUpdate) {
                    this.props.onDidUpdate()
                }
            },
            error => this.setState({ loading: false, errorDescription: error.message })
        )
    }
}

interface Props extends RouteComponentProps<{}> {
    user: GQL.IUser
}

interface State {
    siteFlags?: SiteFlags
}

/** We fake a XyzConnection type because our GraphQL API doesn't have one (or need one) for user emails. */
interface UserEmailConnection {
    nodes: GQL.IUserEmail[]
    totalCount: number
}

export class UserAccountEmailsPage extends React.Component<Props, State> {
    public state: State = {}

    private userEmailUpdates = new Subject<void>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        eventLogger.logViewEvent('UserAccountEmails')

        this.subscriptions.add(siteFlags.subscribe(siteFlags => this.setState({ siteFlags })))
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        const nodeProps: Pick<UserEmailNodeProps, 'user' | 'onDidUpdate'> = {
            user: this.props.user,
            onDidUpdate: this.onDidUpdateUserEmail,
        }

        return (
            <div className="user-settings-emails-page">
                <PageTitle title="Emails" />
                <h2>Emails</h2>
                {this.state.siteFlags &&
                    !this.state.siteFlags.sendsEmailVerificationEmails && (
                        <div className="alert alert-warning mt-2">
                            Sourcegraph is not configured to send email verifications. Newly added email addresses must
                            be manually verified by a site admin.
                        </div>
                    )}
                <FilteredConnection<GQL.IUserEmail, Pick<UserEmailNodeProps, 'user' | 'onDidUpdate'>>
                    className="list-group list-group-flush mt-3"
                    noun="email address"
                    pluralNoun="email addresses"
                    queryConnection={this.queryUserEmails}
                    nodeComponent={UserEmailNode}
                    nodeComponentProps={nodeProps}
                    updates={this.userEmailUpdates}
                    hideSearch={true}
                    noSummaryIfAllNodesVisible={true}
                    history={this.props.history}
                    location={this.props.location}
                />
                <AddUserEmailForm className="mt-4" user={this.props.user.id} onDidAdd={this.onDidUpdateUserEmail} />
            </div>
        )
    }

    private queryUserEmails = (args: {}): Observable<UserEmailConnection> =>
        queryGraphQL(
            gql`
                query UserEmails($user: ID!) {
                    node(id: $user) {
                        ... on User {
                            emails {
                                email
                                verified
                                verificationPending
                                viewerCanManuallyVerify
                            }
                        }
                    }
                }
            `,
            { user: this.props.user.id }
        ).pipe(
            map(({ data, errors }) => {
                if (!data || !data.node) {
                    throw createAggregateError(errors)
                }
                const user = data.node as GQL.IUser
                if (!user.emails) {
                    throw createAggregateError(errors)
                }
                return {
                    nodes: user.emails,
                    totalCount: user.emails.length,
                }
            })
        )

    private onDidUpdateUserEmail = () => this.userEmailUpdates.next()
}
