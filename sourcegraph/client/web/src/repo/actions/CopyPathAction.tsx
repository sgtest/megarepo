import classNames from 'classnames'
import copy from 'copy-to-clipboard'
import ContentCopyIcon from 'mdi-react/ContentCopyIcon'
import React, { useState, useLayoutEffect } from 'react'
import { useLocation } from 'react-router'

import { Tooltip } from '@sourcegraph/branded/src/components/tooltip/Tooltip'
import { useRedesignToggle } from '@sourcegraph/shared/src/util/useRedesignToggle'

import { eventLogger } from '../../tracking/eventLogger'
import { parseBrowserRepoURL } from '../../util/url'

import styles from './CopyPathAction.module.scss'

/**
 * A repository header action that copies the current page's repository or file path to the clipboard.
 */
export const CopyPathAction: React.FunctionComponent = () => {
    const [isRedesignEnabled] = useRedesignToggle()
    const location = useLocation()
    const [copied, setCopied] = useState(false)

    useLayoutEffect(() => {
        Tooltip.forceUpdate()
    }, [copied])

    const onClick = (event: React.MouseEvent<HTMLButtonElement>): void => {
        event.preventDefault()
        eventLogger.log('CopyFilePath')
        const { repoName, filePath } = parseBrowserRepoURL(location.pathname)
        copy(filePath || repoName) // copy the file path if present; else it's the repo path.
        setCopied(true)

        setTimeout(() => {
            setCopied(false)
        }, 1000)
    }

    return (
        <button
            type="button"
            className={classNames('btn btn-icon', isRedesignEnabled && 'btn-sm p-2', !isRedesignEnabled && 'my-2')}
            data-tooltip={copied ? 'Copied!' : 'Copy path to clipboard'}
            aria-label="Copy path"
            onClick={onClick}
        >
            <ContentCopyIcon className={classNames('icon-inline', styles.copyIcon)} />
        </button>
    )
}
