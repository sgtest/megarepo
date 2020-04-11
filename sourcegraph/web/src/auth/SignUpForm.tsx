import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import HelpCircleOutlineIcon from 'mdi-react/HelpCircleOutlineIcon'
import * as React from 'react'
import { from, Subscription } from 'rxjs'
import { asError } from '../../../shared/src/util/errors'
import { Form } from '../components/Form'
import { eventLogger } from '../tracking/eventLogger'
import { enterpriseTrial, signupTerms } from '../util/features'
import { EmailInput, PasswordInput, UsernameInput } from './SignInSignUpCommon'
import { ErrorAlert } from '../components/alerts'
import classNames from 'classnames'

export interface SignUpArgs {
    email: string
    username: string
    password: string
    requestedTrial: boolean
}

interface SignUpFormProps {
    className?: string

    /** Called to perform the signup on the server. */
    doSignUp: (args: SignUpArgs) => Promise<void>

    buttonLabel?: string
}

interface SignUpFormState {
    email: string
    username: string
    password: string
    error?: Error
    loading: boolean
    requestedTrial: boolean
}

export class SignUpForm extends React.Component<SignUpFormProps, SignUpFormState> {
    private subscriptions = new Subscription()

    constructor(props: SignUpFormProps) {
        super(props)
        this.state = {
            email: '',
            username: '',
            password: '',
            loading: false,
            requestedTrial: false,
        }
    }

    public render(): JSX.Element | null {
        return (
            <Form
                className={classNames('signin-signup-form', 'e2e-signup-form', this.props.className)}
                onSubmit={this.handleSubmit}
            >
                {this.state.error && <ErrorAlert className="mb-3" error={this.state.error} />}
                <div className="form-group">
                    <EmailInput
                        className="signin-signup-form__input"
                        onChange={this.onEmailFieldChange}
                        required={true}
                        value={this.state.email}
                        disabled={this.state.loading}
                        autoFocus={true}
                    />
                </div>
                <div className="form-group">
                    <UsernameInput
                        className="signin-signup-form__input"
                        onChange={this.onUsernameFieldChange}
                        value={this.state.username}
                        required={true}
                        disabled={this.state.loading}
                    />
                </div>
                <div className="form-group">
                    <PasswordInput
                        className="signin-signup-form__input"
                        onChange={this.onPasswordFieldChange}
                        value={this.state.password}
                        required={true}
                        disabled={this.state.loading}
                        autoComplete="new-password"
                    />
                </div>
                {enterpriseTrial && (
                    <div className="form-group">
                        <div className="form-check">
                            <label className="form-check-label">
                                <input
                                    className="form-check-input"
                                    type="checkbox"
                                    onChange={this.onRequestTrialFieldChange}
                                />
                                Try Sourcegraph Enterprise free for 30 days{' '}
                                {/* eslint-disable-next-line react/jsx-no-target-blank */}
                                <a target="_blank" rel="noopener" href="https://about.sourcegraph.com/pricing">
                                    <HelpCircleOutlineIcon className="icon-inline" />
                                </a>
                            </label>
                        </div>
                    </div>
                )}
                <div className="form-group mb-0">
                    <button className="btn btn-primary btn-block" type="submit" disabled={this.state.loading}>
                        {this.state.loading ? (
                            <LoadingSpinner className="icon-inline" />
                        ) : (
                            this.props.buttonLabel || 'Sign up'
                        )}
                    </button>
                </div>
                {window.context.sourcegraphDotComMode && (
                    <p className="mt-1 mb-0">
                        Create a public account to search/navigate open-source code and manage Sourcegraph
                        subscriptions.
                    </p>
                )}
                {signupTerms && (
                    <p className="mt-1 mb-0">
                        <small className="form-text text-muted">
                            By signing up, you agree to our
                            {/* eslint-disable-next-line react/jsx-no-target-blank */}
                            <a href="https://about.sourcegraph.com/terms" target="_blank" rel="noopener">
                                Terms of Service
                            </a>{' '}
                            and {/* eslint-disable-next-line react/jsx-no-target-blank */}
                            <a href="https://about.sourcegraph.com/privacy" target="_blank" rel="noopener">
                                Privacy Policy
                            </a>
                            .
                        </small>
                    </p>
                )}
            </Form>
        )
    }

    private onEmailFieldChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
        this.setState({ email: e.target.value })
    }

    private onUsernameFieldChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
        this.setState({ username: e.target.value })
    }

    private onPasswordFieldChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
        this.setState({ password: e.target.value })
    }

    private onRequestTrialFieldChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
        this.setState({ requestedTrial: e.target.checked })
    }

    private handleSubmit = (event: React.FormEvent<HTMLFormElement>): void => {
        event.preventDefault()
        if (this.state.loading) {
            return
        }

        this.setState({ loading: true })
        this.subscriptions.add(
            from(
                this.props
                    .doSignUp({
                        email: this.state.email,
                        username: this.state.username,
                        password: this.state.password,
                        requestedTrial: this.state.requestedTrial,
                    })
                    .catch(error => this.setState({ error: asError(error), loading: false }))
            ).subscribe()
        )
        eventLogger.log('InitiateSignUp')
    }
}
