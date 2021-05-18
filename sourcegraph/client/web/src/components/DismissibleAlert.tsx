import CloseIcon from 'mdi-react/CloseIcon'
import * as React from 'react'

interface Props {
    /** used to build the key that represents the alert in local storage */
    partialStorageKey: string

    /** class name to be applied to the alert */
    className: string
}

/**
 * A global site alert that can be dismissed. Once dismissed, it is never shown
 * again.
 */
export const DismissibleAlert: React.FunctionComponent<Props> = ({ partialStorageKey, className, children }) => {
    const [dismissed, setDismissed] = React.useState<boolean>(isAlertDismissed(partialStorageKey))

    const onDismiss = React.useCallback(() => {
        dismissAlert(partialStorageKey)
        setDismissed(true)
    }, [partialStorageKey])

    if (dismissed) {
        return null
    }
    return (
        <div className={`alert dismissible-alert ${className}`}>
            <div className="dismissible-alert__content">{children}</div>
            <button type="button" className="btn btn-icon" onClick={onDismiss}>
                <CloseIcon className="icon-inline" />
            </button>
        </div>
    )
}

export function dismissAlert(key: string): void {
    localStorage.setItem(storageKeyForPartial(key), 'true')
}

export function isAlertDismissed(key: string): boolean {
    return localStorage.getItem(storageKeyForPartial(key)) === 'true'
}

function storageKeyForPartial(partialStorageKey: string): string {
    return `DismissibleAlert/${partialStorageKey}/dismissed`
}
