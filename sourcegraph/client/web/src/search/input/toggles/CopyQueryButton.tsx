import copy from 'copy-to-clipboard'
import ClipboardOutlineIcon from 'mdi-react/ClipboardOutlineIcon'
import { Shortcut } from '@slimsag/react-shortcuts'
import { Tooltip } from '../../../../../branded/src/components/tooltip/Tooltip'
import React, { useCallback, useRef, useEffect } from 'react'
import classNames from 'classnames'
import { Observable, merge, of } from 'rxjs'
import { tap, switchMapTo, startWith, delay } from 'rxjs/operators'
import { useEventObservable } from '../../../../../shared/src/util/useObservable'
import { KeyboardShortcut } from '../../../../../shared/src/keyboardShortcuts'

interface Props {
    fullQuery: string
    className?: string
    isMacPlatform: boolean
    keyboardShortcutForFullCopy: KeyboardShortcut
}

/**
 * A repository header action that copies the current page's URL to the clipboard.
 */
export const CopyQueryButton: React.FunctionComponent<Props> = (props: Props) => {
    // Convoluted, but using props.fullQuery directly in the copyFullQuery callback does not work, since
    // props.fullQuery is not refrenced during the render and it is not updated within the callback.
    const fullQueryReference = useRef<string>('')
    useEffect(() => {
        fullQueryReference.current = props.fullQuery
    }, [props.fullQuery])

    const copyFullQuery = useCallback((): boolean => copy(fullQueryReference.current), [fullQueryReference])

    const [nextClick, copied] = useEventObservable(
        useCallback(
            (clicks: Observable<React.MouseEvent>) =>
                clicks.pipe(
                    tap(copyFullQuery),
                    switchMapTo(merge(of(true), of(false).pipe(delay(2000)))),
                    tap(() => Tooltip.forceUpdate()),
                    startWith(false)
                ),
            [copyFullQuery]
        )
    )

    const copyFullQueryTooltip = `Copy full query\n${props.isMacPlatform ? '⌘' : 'Ctrl'}+⇧+C`
    return (
        <>
            <button
                type="button"
                className={classNames('btn btn-icon icon-inline  btn-link-sm', props.className)}
                data-tooltip={copied ? 'Copied!' : copyFullQueryTooltip}
                onClick={nextClick}
            >
                <ClipboardOutlineIcon size={16} className="icon-inline" />
            </button>
            {props.keyboardShortcutForFullCopy.keybindings.map((keybinding, index) => (
                <Shortcut key={index} {...keybinding} onMatch={copyFullQuery} allowDefault={false} ignoreInput={true} />
            ))}
        </>
    )
}
