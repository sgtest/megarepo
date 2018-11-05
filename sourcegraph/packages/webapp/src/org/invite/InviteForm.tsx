import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { upperFirst } from 'lodash'
import AddIcon from 'mdi-react/AddIcon'
import CloseIcon from 'mdi-react/CloseIcon'
import EmailOpenOutlineIcon from 'mdi-react/EmailOpenOutlineIcon'
import * as React from 'react'
import { Link } from 'react-router-dom'
import { merge, Observable, of, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, filter, map, mergeMap, startWith, tap, withLatestFrom } from 'rxjs/operators'
import { gql, mutateGraphQL } from '../../backend/graphql'
import * as GQL from '../../backend/graphqlschema'
import { CopyableText } from '../../components/CopyableText'
import { DismissibleAlert } from '../../components/DismissibleAlert'
import { Form } from '../../components/Form'
import { eventLogger } from '../../tracking/eventLogger'
import { createAggregateError } from '../../util/errors'

function inviteUserToOrganization(
    username: string,
    organization: GQL.ID
): Observable<GQL.IInviteUserToOrganizationResult> {
    return mutateGraphQL(
        gql`
            mutation InviteUserToOrganization($organization: ID!, $username: String!) {
                inviteUserToOrganization(organization: $organization, username: $username) {
                    sentInvitationEmail
                    invitationURL
                }
            }
        `,
        {
            username,
            organization,
        }
    ).pipe(
        map(({ data, errors }) => {
            const eventData = {
                organization: {
                    invite: {
                        username,
                    },
                    org_id: organization,
                },
            }
            if (!data || !data.inviteUserToOrganization || (errors && errors.length > 0)) {
                eventLogger.log('InviteOrgMemberFailed', eventData)
                throw createAggregateError(errors)
            }
            eventLogger.log('OrgMemberInvited', eventData)
            return data.inviteUserToOrganization
        })
    )
}

function addUserToOrganization(username: string, organization: GQL.ID): Observable<void> {
    return mutateGraphQL(
        gql`
            mutation AddUserToOrganization($organization: ID!, $username: String!) {
                addUserToOrganization(organization: $organization, username: $username) {
                    alwaysNil
                }
            }
        `,
        {
            username,
            organization,
        }
    ).pipe(
        map(({ data, errors }) => {
            if (!data || !data.addUserToOrganization || (errors && errors.length > 0)) {
                eventLogger.log('AddOrgMemberFailed')
                throw createAggregateError(errors)
            }
            eventLogger.log('OrgMemberAdded')
        })
    )
}

const emailInvitesEnabled = window.context.emailEnabled

const InvitedNotification: React.SFC<{
    className: string
    username: string
    sentInvitationEmail: boolean
    invitationURL: string
    onDismiss: () => void
}> = ({ className, username, sentInvitationEmail, invitationURL, onDismiss }) => (
    <div className={`${className} invited-notification`}>
        <div className="invited-notification__message">
            {sentInvitationEmail ? (
                <>
                    Invitation sent to {username}. You can also send {username} the invitation link directly:
                </>
            ) : (
                <>Generated invitation link. Copy and send it to {username}:</>
            )}
            <CopyableText text={invitationURL} size={40} className="mt-2" />
        </div>
        <button className="btn btn-icon" title="Dismiss" onClick={onDismiss}>
            <CloseIcon className="icon-inline" />
        </button>
    </div>
)

interface Props {
    orgID: string
    authenticatedUser: GQL.IUser | null

    /** Called when the organization members list changes. */
    onDidUpdateOrganizationMembers: () => void

    onOrganizationUpdate: () => void
}

interface SubmittedInvite extends Pick<GQL.IInviteUserToOrganizationResult, 'sentInvitationEmail' | 'invitationURL'> {
    username: string
}

interface State {
    username: string

    /** Loading state (undefined means not loading). */
    loading?: 'inviteUserToOrganization' | 'addUserToOrganization'

    invited?: SubmittedInvite[]
    error?: Error
}

export class InviteForm extends React.PureComponent<Props, State> {
    public state: State = { username: '' }

    private submits = new Subject<React.FormEvent<HTMLFormElement>>()
    private inviteClicks = new Subject<React.MouseEvent<HTMLButtonElement>>()
    private usernameChanges = new Subject<string>()
    private componentUpdates = new Subject<Props>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        const orgChanges = this.componentUpdates.pipe(distinctUntilChanged((a, b) => a.orgID !== b.orgID))

        type Update = (prevState: State) => State

        this.subscriptions.add(this.usernameChanges.subscribe(username => this.setState({ username })))

        // Invite clicks.
        this.subscriptions.add(
            merge(this.submits.pipe(filter(() => !this.viewerCanAddUserToOrganization)), this.inviteClicks)
                .pipe(
                    tap(e => e.preventDefault()),
                    withLatestFrom(orgChanges, this.usernameChanges),
                    tap(([, orgId, username]) =>
                        eventLogger.log('InviteOrgMemberClicked', {
                            organization: {
                                invite: {
                                    username,
                                },
                                org_id: orgId,
                            },
                        })
                    ),
                    mergeMap(([, { orgID }, username]) =>
                        inviteUserToOrganization(username, orgID).pipe(
                            tap(() => this.props.onOrganizationUpdate()),
                            tap(() => this.usernameChanges.next('')),
                            mergeMap(({ sentInvitationEmail, invitationURL }) =>
                                // Reset email, reenable submit button, flash "invited" text
                                of(
                                    (state: State): State => ({
                                        ...state,
                                        loading: undefined,
                                        error: undefined,
                                        username: '',
                                        invited: [
                                            ...(state.invited || []),
                                            { username, sentInvitationEmail, invitationURL },
                                        ],
                                    })
                                )
                            ),
                            // Disable button while loading
                            startWith<Update>(
                                (state: State): State => ({
                                    ...state,
                                    loading: 'inviteUserToOrganization',
                                })
                            ),
                            catchError(error => [(state: State): State => ({ ...state, loading: undefined, error })])
                        )
                    )
                )
                .subscribe(stateUpdate => this.setState(stateUpdate), err => console.error(err))
        )

        // Adds.
        this.subscriptions.add(
            this.submits
                .pipe(filter(() => this.viewerCanAddUserToOrganization))
                .pipe(
                    tap(e => e.preventDefault()),
                    withLatestFrom(orgChanges, this.usernameChanges),
                    mergeMap(([, { orgID }, username]) =>
                        addUserToOrganization(username, orgID).pipe(
                            tap(() => this.props.onDidUpdateOrganizationMembers()),
                            tap(() => this.usernameChanges.next('')),
                            mergeMap(() =>
                                // Reset email, reenable submit button, flash "invited" text
                                of(
                                    (state: State): State => ({
                                        ...state,
                                        loading: undefined,
                                        error: undefined,
                                        username: '',
                                    })
                                )
                            ),
                            // Disable button while loading
                            startWith<Update>(
                                (state: State): State => ({
                                    ...state,
                                    loading: 'addUserToOrganization',
                                })
                            ),
                            catchError(error => [(state: State): State => ({ ...state, loading: undefined, error })])
                        )
                    )
                )
                .subscribe(stateUpdate => this.setState(stateUpdate), err => console.error(err))
        )

        this.componentUpdates.next(this.props)
    }

    public componentWillReceiveProps(nextProps: Props): void {
        this.componentUpdates.next(nextProps)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    private get viewerCanAddUserToOrganization(): boolean {
        return !!this.props.authenticatedUser && this.props.authenticatedUser.siteAdmin
    }

    public render(): JSX.Element | null {
        const viewerCanAddUserToOrganization = this.viewerCanAddUserToOrganization

        return (
            <div className="invite-form">
                <div className="card invite-form__container">
                    <div className="card-body">
                        <h4 className="card-title">
                            {this.viewerCanAddUserToOrganization ? 'Add or invite member' : 'Invite member'}
                        </h4>
                        <Form className="form-inline align-items-start" onSubmit={this.onSubmit}>
                            <label className="sr-only" htmlFor="invite-form__username">
                                Username
                            </label>
                            <input
                                type="text"
                                className="form-control mb-2 mr-sm-2"
                                id="invite-form__username"
                                placeholder="Username"
                                onChange={this.onUsernameChange}
                                value={this.state.username}
                                autoComplete="off"
                                autoCapitalize="off"
                                autoCorrect="off"
                                required={true}
                                spellCheck={false}
                                size={30}
                            />
                            {viewerCanAddUserToOrganization && (
                                <button
                                    type="submit"
                                    disabled={!!this.state.loading}
                                    className="btn btn-primary mb-2 mr-sm-2"
                                    data-tooltip="Add immediately without sending invitation (site admins only)"
                                >
                                    {this.state.loading === 'addUserToOrganization' ? (
                                        <LoadingSpinner className="icon-inline" />
                                    ) : (
                                        <AddIcon className="icon-inline" />
                                    )}{' '}
                                    Add member
                                </button>
                            )}
                            {(emailInvitesEnabled || !this.viewerCanAddUserToOrganization) && (
                                <div className="form-group flex-column mb-2 mr-sm-2">
                                    <button
                                        type={viewerCanAddUserToOrganization ? 'button' : 'submit'}
                                        disabled={!!this.state.loading}
                                        className={`btn ${
                                            viewerCanAddUserToOrganization ? 'btn-secondary' : 'btn-primary'
                                        }`}
                                        data-tooltip={
                                            emailInvitesEnabled
                                                ? 'Send invitation email with link to join this organization'
                                                : 'Generate invitation link to manually send to user'
                                        }
                                        onClick={viewerCanAddUserToOrganization ? this.onInviteClick : undefined}
                                    >
                                        {this.state.loading === 'inviteUserToOrganization' ? (
                                            <LoadingSpinner className="icon-inline" />
                                        ) : (
                                            <EmailOpenOutlineIcon className="icon-inline" />
                                        )}{' '}
                                        {emailInvitesEnabled
                                            ? this.viewerCanAddUserToOrganization
                                                ? 'Send invitation to join'
                                                : 'Send invitation'
                                            : 'Generate invitation link'}
                                    </button>
                                </div>
                            )}
                        </Form>
                    </div>
                </div>
                {this.props.authenticatedUser &&
                    this.props.authenticatedUser.siteAdmin &&
                    !window.context.emailEnabled && (
                        <DismissibleAlert className="alert-info" partialStorageKey="org-invite-email-config">
                            <p className=" mb-0">
                                Set <code>email.smtp</code> in{' '}
                                <Link to="/site-admin/configuration">site configuration</Link> to send email
                                notfications about invitations.
                            </p>
                        </DismissibleAlert>
                    )}
                {this.state.invited &&
                    this.state.invited.map(({ username, sentInvitationEmail, invitationURL }, i) => (
                        <InvitedNotification
                            key={i}
                            className="alert alert-success invite-form__alert"
                            username={username}
                            sentInvitationEmail={sentInvitationEmail}
                            invitationURL={invitationURL}
                            // tslint:disable-next-line:jsx-no-lambda
                            onDismiss={() => this.dismissNotification(i)}
                        />
                    ))}
                {this.state.error && (
                    <div className="invite-form__alert alert alert-danger">
                        Error: {upperFirst(this.state.error.message)}
                    </div>
                )}
            </div>
        )
    }

    private onUsernameChange: React.ChangeEventHandler<HTMLInputElement> = e =>
        this.usernameChanges.next(e.currentTarget.value)
    private onSubmit: React.FormEventHandler<HTMLFormElement> = e => this.submits.next(e)
    private onInviteClick: React.MouseEventHandler<HTMLButtonElement> = e => this.inviteClicks.next(e)

    private dismissNotification = (i: number): void => {
        this.setState(prevState => ({ invited: (prevState.invited || []).filter((_, j) => i !== j) }))
    }
}
