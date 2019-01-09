import * as React from 'react'
import { from, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, map, scan, switchMap } from 'rxjs/operators'
import { Progress } from 'sourcegraph'
import { MessageType } from '../api/client/services/notifications'
import { renderMarkdown } from '../util/markdown'
import { Notification } from './notification'

interface Props {
    notification: Notification
    onDismiss: (notification: Notification) => void
    className?: string
}

interface State {
    progress?: Required<Progress>
}

/**
 * A notification message displayed in a {@link module:./Notifications.Notifications} component.
 */
export class NotificationItem extends React.PureComponent<Props, State> {
    private componentUpdates = new Subject<Props>()
    private subscription = new Subscription()
    constructor(props: Props) {
        super(props)
        this.state = {
            progress: props.notification.progress && {
                percentage: 0,
                message: '',
            },
        }
    }
    public componentDidMount(): void {
        this.subscription.add(
            this.componentUpdates
                .pipe(
                    map(props => props.notification.progress),
                    distinctUntilChanged(),
                    switchMap(progress =>
                        from(progress || []).pipe(
                            // Hide progress bar and update message if error occured
                            // Merge new progress updates with previous
                            scan<Progress, Required<Progress>>(
                                (current, { message = current.message, percentage = current.percentage }) => ({
                                    message,
                                    percentage,
                                }),
                                {
                                    percentage: 0,
                                    message: '',
                                }
                            ),
                            catchError(() => [undefined])
                        )
                    )
                )
                .subscribe(progress => {
                    this.setState({ progress })
                })
        )
        this.componentUpdates.next(this.props)
    }
    public componentDidUpdate(): void {
        this.componentUpdates.next(this.props)
    }
    public componentWillUnmount(): void {
        this.subscription.unsubscribe()
    }
    public render(): JSX.Element | null {
        const bootstrapClass = getBootstrapClass(this.props.notification.type)
        return (
            <div
                className={`sourcegraph-notification-item alert alert-${bootstrapClass} p-0 ${this.props.className ||
                    ''}`}
            >
                <div className="w-100 d-flex align-items-start">
                    <div className="p-2 flex-grow-1 mw-100">
                        <div
                            className="sourcegraph-notification-item__title"
                            dangerouslySetInnerHTML={{ __html: renderMarkdown(this.props.notification.message || '') }}
                        />
                        {this.state.progress && (
                            <div
                                className="sourcegraph-notification-item__content"
                                dangerouslySetInnerHTML={{
                                    __html: renderMarkdown(this.state.progress.message),
                                }}
                            />
                        )}
                    </div>
                    {(!this.props.notification.progress || !this.state.progress) && (
                        <button
                            type="button"
                            className="sourcegraph-notification-item__close close p-2 flex-grow-0 flex-shrink-0"
                            onClick={this.onDismiss}
                            aria-label="Close"
                        >
                            <span aria-hidden="true">&times;</span>
                        </button>
                    )}
                </div>
                {this.props.notification.progress &&
                    this.state.progress && (
                        <div className="progress">
                            <div
                                className={`sourcegraph-notification-item__progressbar progress-bar`}
                                // tslint:disable-next-line:jsx-ban-props
                                style={{ width: this.state.progress.percentage + '%' }}
                            />
                        </div>
                    )}
            </div>
        )
    }

    private onDismiss = () => this.props.onDismiss(this.props.notification)
}

/**
 * @return The Bootstrap class that corresponds to {@link type}.
 */
function getBootstrapClass(type: MessageType | undefined): string {
    switch (type) {
        case MessageType.Error:
            return 'danger'
        case MessageType.Warning:
            return 'warning'
        case MessageType.Info:
            return 'info'
        case MessageType.Log:
        default:
            return 'secondary'
    }
}
