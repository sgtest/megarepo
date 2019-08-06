import * as React from 'react'
import { Markdown } from '../../../shared/src/components/Markdown'
import { isSettingsValid, SettingsCascadeProps } from '../../../shared/src/settings/settings'
import { renderMarkdown } from '../../../shared/src/util/markdown'
import { DismissibleAlert } from '../components/DismissibleAlert'
import { Notice, Settings } from '../schema/settings.schema'

const NoticeAlert: React.FunctionComponent<{ notice: Notice; className?: string }> = ({ notice, className = '' }) => {
    const content = <Markdown dangerousInnerHTML={renderMarkdown(notice.message)} />
    const baseClassName = notice.location === 'top' ? 'alert-info' : 'bg-transparent border'
    return notice.dismissible ? (
        <DismissibleAlert className={`${baseClassName} ${className}`} partialStorageKey={`notice.${notice.message}`}>
            {content}
        </DismissibleAlert>
    ) : (
        <div className={`alert ${baseClassName} ${className}`}>{content}</div>
    )
}

interface Props extends SettingsCascadeProps {
    className?: string

    /** Apply this class name to each notice (alongside .alert). */
    alertClassName?: string

    /** Display notices for this location. */
    location: Notice['location']
}

/**
 * Displays notices from settings for a specific location.
 */
export const Notices: React.FunctionComponent<Props> = ({
    className = '',
    alertClassName,
    settingsCascade,
    location,
}) => {
    if (
        !isSettingsValid<Settings>(settingsCascade) ||
        !settingsCascade.final.notices ||
        !Array.isArray(settingsCascade.final.notices)
    ) {
        return null
    }
    const notices = settingsCascade.final.notices.filter(n => n.location === location)
    if (notices.length === 0) {
        return null
    }
    return (
        <div className={`notices ${className}`}>
            {notices.map((notice, i) => (
                <NoticeAlert key={i} className={alertClassName} notice={notice} />
            ))}
        </div>
    )
}
