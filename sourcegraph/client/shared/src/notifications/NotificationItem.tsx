import * as React from 'react'

import classNames from 'classnames'
import { from, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, map, scan, switchMap } from 'rxjs/operators'

import { renderMarkdown } from '@sourcegraph/common'
import { Alert } from '@sourcegraph/wildcard'

import type { NotificationType, Progress } from '../codeintel/legacy-extensions/api'

import { Notification } from './notification'

import styles from './NotificationItem.module.scss'

export interface UnbrandedNotificationItemStyleProps {
    notificationItemClassNames: Record<NotificationType, string>
}

export interface NotificationItemProps {
    notification: Notification
    onDismiss: (notification: Notification) => void
    className?: string
    notificationItemStyleProps: UnbrandedNotificationItemStyleProps
}

interface NotificationItemState {
    progress?: Required<Progress>
}

/**
 * A notification message displayed in a {@link module:./Notifications.Notifications} component.
 */
export class NotificationItem extends React.PureComponent<NotificationItemProps, NotificationItemState> {
    private componentUpdates = new Subject<NotificationItemProps>()
    private subscription = new Subscription()
    constructor(props: NotificationItemProps) {
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
                            // Hide progress bar and update message if error occurred
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
        const baseAlertClassName = classNames(styles.sourcegraphNotificationItem, this.props.className)

        const { notificationItemStyleProps } = this.props
        const alertProps = {
            className: classNames(
                baseAlertClassName,
                notificationItemStyleProps.notificationItemClassNames[this.props.notification.type]
            ),
        }

        return (
            <Alert {...alertProps}>
                <div className={styles.bodyContainer}>
                    <div className={styles.body}>
                        <div
                            className={styles.title}
                            dangerouslySetInnerHTML={{
                                __html: renderMarkdown(this.props.notification.message || '', {
                                    allowDataUriLinksAndDownloads: true,
                                }),
                            }}
                        />
                        {this.state.progress && (
                            <div
                                className={styles.content}
                                dangerouslySetInnerHTML={{
                                    __html: renderMarkdown(this.state.progress.message),
                                }}
                            />
                        )}
                    </div>
                    {(!this.props.notification.progress || !this.state.progress) && (
                        <button
                            type="button"
                            className={classNames('close', styles.close)}
                            onClick={this.onDismiss}
                            aria-label="Close"
                        >
                            <span aria-hidden="true">&times;</span>
                        </button>
                    )}
                </div>
                {this.props.notification.progress && this.state.progress && (
                    <div className={classNames('progress', styles.progress)}>
                        <div
                            className={classNames('progress-bar', styles.progressbar)}
                            // eslint-disable-next-line react/forbid-dom-props
                            style={{ width: `${this.state.progress.percentage}%` }}
                        />
                    </div>
                )}
            </Alert>
        )
    }

    private onDismiss = (): void => this.props.onDismiss(this.props.notification)
}
