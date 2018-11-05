import { upperFirst } from 'lodash'
import * as React from 'react'

export const BaseActionContainer: React.SFC<{
    title: React.ReactFragment
    description: React.ReactFragment
    action: React.ReactFragment
    details?: React.ReactFragment
}> = ({ title, description, action, details }) => (
    <div className="action-container">
        <div className="action-container__row">
            <div className="action-container__description">
                <h4 className="action-container__title">{title}</h4>
                {description}
            </div>
            <div className="action-container__btn-container">{action}</div>
        </div>
        {details && <div className="action-container__row">{details}</div>}
    </div>
)

interface Props {
    title: React.ReactFragment
    description: React.ReactFragment
    buttonClassName?: string
    buttonLabel: React.ReactFragment
    buttonSubtitle?: string
    buttonDisabled?: boolean
    info?: React.ReactNode

    /** The message to briefly display below the button when the action is successful. */
    flashText?: string

    run: () => Promise<void>
}

interface State {
    loading: boolean
    flash: boolean
    error?: string
}

/**
 * Displays an action button in a container with a title and description.
 */
export class ActionContainer extends React.PureComponent<Props, State> {
    public state: State = {
        loading: false,
        flash: false,
    }

    private timeoutHandle?: number

    public componentWillUnmount(): void {
        if (this.timeoutHandle) {
            window.clearTimeout(this.timeoutHandle)
        }
    }

    public render(): JSX.Element | null {
        return (
            <BaseActionContainer
                title={this.props.title}
                description={this.props.description}
                action={
                    <>
                        <button
                            className={`btn ${this.props.buttonClassName || 'btn-primary'} action-container__btn`}
                            onClick={this.onClick}
                            data-tooltip={this.props.buttonSubtitle}
                            disabled={this.props.buttonDisabled || this.state.loading}
                        >
                            {this.props.buttonLabel}
                        </button>
                        {this.props.buttonSubtitle && (
                            <div className="action-container__btn-subtitle">
                                <small>{this.props.buttonSubtitle}</small>
                            </div>
                        )}
                        {!this.props.buttonSubtitle &&
                            this.props.flashText && (
                                <div
                                    className={
                                        'action-container__flash' +
                                        (this.state.flash ? ' action-container__flash--visible' : '')
                                    }
                                >
                                    <small>{this.props.flashText}</small>
                                </div>
                            )}
                    </>
                }
                details={
                    <>
                        {this.state.error && (
                            <div className="alert alert-danger mb-0 mt-3">Error: {upperFirst(this.state.error)}</div>
                        )}
                        {!this.state.error && this.props.info}
                    </>
                }
            />
        )
    }

    private onClick = () => {
        this.setState({
            error: undefined,
            loading: true,
        })

        this.props.run().then(
            () => {
                this.setState({ loading: false, flash: true })
                if (typeof this.timeoutHandle === 'number') {
                    window.clearTimeout(this.timeoutHandle)
                }
                this.timeoutHandle = window.setTimeout(() => this.setState({ flash: false }), 1000)
            },
            err => this.setState({ loading: false, error: err.message })
        )
    }
}
