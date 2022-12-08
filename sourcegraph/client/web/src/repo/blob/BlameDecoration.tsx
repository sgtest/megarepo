import { useCallback, useEffect, useMemo } from 'react'

import classNames from 'classnames'
import { History } from 'history'
import SourceCommitIcon from 'mdi-react/SourceCommitIcon'
import { BehaviorSubject } from 'rxjs'

import { createLinkClickHandler } from '@sourcegraph/shared/src/components/utils/linkClickHandler'
import {
    createRectangle,
    Icon,
    Link,
    Popover,
    PopoverContent,
    PopoverOpenEvent,
    PopoverTrigger,
    Position,
    useObservable,
} from '@sourcegraph/wildcard'

import { eventLogger } from '../../tracking/eventLogger'
import { replaceRevisionInURL } from '../../util/url'
import { BlameHunk } from '../blame/useBlameHunks'

import styles from './BlameDecoration.module.scss'

const currentPopoverId = new BehaviorSubject<string | null>(null)
let closeTimeoutId: NodeJS.Timeout | null = null
const resetCloseTimeout = (): void => {
    if (closeTimeoutId) {
        clearTimeout(closeTimeoutId)
        closeTimeoutId = null
    }
}
let openTimeoutId: NodeJS.Timeout | null = null
const resetOpenTimeout = (): void => {
    if (openTimeoutId) {
        clearTimeout(openTimeoutId)
        openTimeoutId = null
    }
}
const resetAllTimeouts = (): void => {
    resetOpenTimeout()
    resetCloseTimeout()
}

const usePopover = ({
    id,
    timeout,
    onOpen,
    onClose,
}: {
    id: string
    timeout: number
    onOpen?: () => void
    onClose?: () => void
}): {
    isOpen: boolean
    open: () => void
    close: () => void
    openWithTimeout: () => void
    closeWithTimeout: () => void
} => {
    const popoverId = useObservable(currentPopoverId)

    const isOpen = popoverId === id
    useEffect(() => {
        if (isOpen) {
            onOpen?.()
        }

        return () => {
            if (isOpen) {
                onClose?.()
            }
        }
    }, [isOpen, onOpen, onClose])

    const open = useCallback(() => {
        resetCloseTimeout()
        currentPopoverId.next(id)
    }, [id])

    const close = useCallback(() => {
        if (currentPopoverId.getValue() === id) {
            currentPopoverId.next(null)
        }
    }, [id])

    const openWithTimeout = useCallback(() => {
        if (currentPopoverId.getValue() === null) {
            open()
            return
        }
        resetOpenTimeout()
        openTimeoutId = setTimeout(open, timeout)
    }, [open, timeout])

    const closeWithTimeout = useCallback(() => {
        resetCloseTimeout()
        closeTimeoutId = setTimeout(close, timeout)
    }, [close, timeout])

    return { isOpen, open, close, openWithTimeout, closeWithTimeout }
}

export const BlameDecoration: React.FunctionComponent<{
    line: number // 1-based line number
    blameHunk?: BlameHunk
    history: History
    onSelect?: (line: number) => void
    onDeselect?: (line: number) => void
}> = ({ line, blameHunk, history, onSelect, onDeselect }) => {
    const id = line?.toString() || ''
    const onOpen = useCallback(() => {
        onSelect?.(line)
        eventLogger.log('GitBlamePopupViewed')
    }, [onSelect, line])
    const onClose = useCallback(() => onDeselect?.(line), [onDeselect, line])
    const { isOpen, open, close, closeWithTimeout, openWithTimeout } = usePopover({
        id,
        timeout: 250,
        onOpen,
        onClose,
    })

    const onPopoverOpenChange = useCallback(
        (event: PopoverOpenEvent) => (event.isOpen ? close() : open()),
        [close, open]
    )

    // Prevent hitting the backend (full page reloads) for links that stay inside the app.
    const handleParentCommitLinkClick = useMemo(() => createLinkClickHandler(history), [history])

    if (!blameHunk) {
        return null
    }

    return (
        <Popover isOpen={isOpen} onOpenChange={onPopoverOpenChange} key={id}>
            <PopoverTrigger
                as={Link}
                to={blameHunk.displayInfo.linkURL}
                target="_blank"
                rel="noreferrer noopener"
                className={classNames(styles.popoverTrigger, 'px-2')}
                onFocus={open}
                onBlur={close}
                onMouseEnter={openWithTimeout}
                onMouseLeave={closeWithTimeout}
            >
                <span
                    className={styles.content}
                    data-line-decoration-attachment-content={true}
                    data-contents={blameHunk.displayInfo.message}
                />
            </PopoverTrigger>

            <PopoverContent
                constraintPadding={createRectangle(150, 0, 0, 0)}
                position={Position.topStart}
                focusLocked={false}
                returnTargetFocus={false}
                onMouseEnter={resetAllTimeouts}
                onMouseLeave={close}
                className={styles.popoverContent}
            >
                <div className="py-1">
                    <div className={classNames(styles.head, 'px-3 my-2')}>
                        <span className={styles.author}>{blameHunk.displayInfo.displayName}</span>{' '}
                        {blameHunk.displayInfo.timestampString}
                    </div>
                    <hr className={classNames(styles.separator, 'm-0')} />
                    <div className={classNames('d-flex align-items-center', styles.block, styles.body)}>
                        <Icon
                            aria-hidden={true}
                            as={SourceCommitIcon}
                            className={classNames('mr-2 flex-shrink-0', styles.icon)}
                        />
                        <Link
                            to={blameHunk.displayInfo.linkURL}
                            target="_blank"
                            rel="noreferrer noopener"
                            className={styles.link}
                            onClick={logCommitClick}
                        >
                            {blameHunk.message}
                        </Link>
                    </div>
                    {blameHunk.commit.parents.length > 0 && (
                        <>
                            <hr className={classNames(styles.separator, 'm-0')} />
                            <div className={classNames('px-3', styles.block)}>
                                <Link
                                    to={
                                        window.location.origin +
                                        replaceRevisionInURL(window.location.href, blameHunk.commit.parents[0].oid)
                                    }
                                    onClick={handleParentCommitLinkClick}
                                    className={styles.footerLink}
                                >
                                    View blame prior to this change
                                </Link>
                            </div>
                        </>
                    )}
                </div>
            </PopoverContent>
        </Popover>
    )
}

const logCommitClick = (): void => {
    eventLogger.log('GitBlamePopupClicked', { target: 'commit' }, { target: 'commit' })
}
