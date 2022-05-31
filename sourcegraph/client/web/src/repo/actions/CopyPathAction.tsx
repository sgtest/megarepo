import React, { useState, useLayoutEffect } from 'react'

import copy from 'copy-to-clipboard'
import ContentCopyIcon from 'mdi-react/ContentCopyIcon'
import { useLocation } from 'react-router'

import { Button, DeprecatedTooltipController, Icon, screenReaderAnnounce } from '@sourcegraph/wildcard'

import { eventLogger } from '../../tracking/eventLogger'
import { parseBrowserRepoURL } from '../../util/url'

import styles from './CopyPathAction.module.scss'

/**
 * A repository header action that copies the current page's repository or file path to the clipboard.
 */
export const CopyPathAction: React.FunctionComponent<React.PropsWithChildren<unknown>> = () => {
    const location = useLocation()
    const [copied, setCopied] = useState(false)

    useLayoutEffect(() => {
        DeprecatedTooltipController.forceUpdate()
    }, [copied])

    const onClick = (event: React.MouseEvent<HTMLButtonElement>): void => {
        event.preventDefault()
        eventLogger.log('CopyFilePath')
        const { repoName, filePath } = parseBrowserRepoURL(location.pathname)
        copy(filePath || repoName) // copy the file path if present; else it's the repo path.
        setCopied(true)
        screenReaderAnnounce('Path copied to clipboard')

        setTimeout(() => {
            setCopied(false)
        }, 1000)
    }

    const label = copied ? 'Copied!' : 'Copy path to clipboard'

    return (
        <Button variant="icon" className="p-2" data-tooltip={label} aria-label={label} onClick={onClick} size="sm">
            <Icon role="img" className={styles.copyIcon} as={ContentCopyIcon} aria-hidden={true} />
        </Button>
    )
}
