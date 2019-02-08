import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as H from 'history'
import { upperFirst } from 'lodash'
import * as React from 'react'
import { Link } from 'react-router-dom'
import { Form } from '../components/Form'
import { eventLogger } from '../tracking/eventLogger'
import { getReturnTo, PasswordInput } from './SignInSignUpCommon'

interface Props {
    location: H.Location
    history: H.History
}

interface State {
    email: string
    password: string
    errorDescription: string
    loading: boolean
}

/**
 * The form for signing in with a username and password.
 */
export class UsernamePasswordSignInForm extends React.Component<Props, State> {
    constructor(props: Props) {
        super(props)
        this.state = {
            email: '',
            password: '',
            errorDescription: '',
            loading: false,
        }
    }

    public render(): JSX.Element | null {
        return (
            <Form className="signin-signup-form signin-form" onSubmit={this.handleSubmit}>
                {window.context.allowSignup ? (
                    <Link className="signin-signup-form__mode" to={`/sign-up${this.props.location.search}`}>
                        Don't have an account? Sign up.
                    </Link>
                ) : (
                    <p className="text-muted">To create an account, contact the site admin.</p>
                )}
                {this.state.errorDescription !== '' && (
                    <div className="alert alert-danger my-2">Error: {upperFirst(this.state.errorDescription)}</div>
                )}
                <div className="form-group">
                    <input
                        className={`form-control signin-signup-form__input`}
                        type="text"
                        placeholder="Username or email"
                        onChange={this.onEmailFieldChange}
                        required={true}
                        value={this.state.email}
                        disabled={this.state.loading}
                        autoCapitalize="off"
                        autoFocus={true}
                        autoComplete="username email"
                    />
                </div>
                <div className="form-group">
                    <PasswordInput
                        className="signin-signup-form__input"
                        onChange={this.onPasswordFieldChange}
                        value={this.state.password}
                        required={true}
                        disabled={this.state.loading}
                        autoComplete="current-password"
                    />
                </div>
                <div className="form-group">
                    <button className="btn btn-primary btn-block" type="submit" disabled={this.state.loading}>
                        Sign in
                    </button>
                    {window.context.resetPasswordEnabled && (
                        <small className="form-text text-muted">
                            <Link to="/password-reset">Forgot password?</Link>
                        </small>
                    )}
                </div>
                {this.state.loading && (
                    <div className="signin-signup-form__loader">
                        <LoadingSpinner className="icon-inline" />
                    </div>
                )}
            </Form>
        )
    }

    private onEmailFieldChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        this.setState({ email: e.target.value })
    }

    private onPasswordFieldChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        this.setState({ password: e.target.value })
    }

    private handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
        event.preventDefault()
        if (this.state.loading) {
            return
        }

        this.setState({ loading: true })
        eventLogger.log('InitiateSignIn')
        fetch('/-/sign-in', {
            credentials: 'same-origin',
            method: 'POST',
            headers: {
                ...window.context.xhrHeaders,
                Accept: 'application/json',
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                email: this.state.email,
                password: this.state.password,
            }),
        })
            .then(resp => {
                if (resp.status === 200) {
                    const returnTo = getReturnTo(this.props.location)
                    window.location.replace(returnTo)
                } else if (resp.status === 401) {
                    throw new Error('User or password was incorrect')
                } else {
                    throw new Error('Unknown Error')
                }
            })
            .catch(err => {
                console.error('auth error: ', err)
                this.setState({ loading: false, errorDescription: (err && err.message) || 'Unknown Error' })
            })
    }
}
