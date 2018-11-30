import { MessageType } from '../api/client/services/notifications'
import { ErrorLike } from '../util/errors'

/**
 * A notification message to display to the user.
 */
export interface Notification {
    /** The message or error of the notification. */
    message: string | ErrorLike

    /**
     * The type of the message.
     *
     * @default MessageType.Info
     */
    type?: MessageType

    /** The source of the notification.  */
    source?: string
}
