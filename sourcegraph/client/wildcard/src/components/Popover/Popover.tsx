import classNames from 'classnames'
import { noop } from 'lodash'
import React, {
    createContext,
    forwardRef,
    MutableRefObject,
    useCallback,
    useContext,
    useMemo,
    useRef,
    useState,
} from 'react'
import FocusLock from 'react-focus-lock'
import { useCallbackRef, useMergeRefs } from 'use-callback-ref'

import { useOnClickOutside, useKeyboard } from '../../hooks'
import { ForwardReferenceComponent } from '../../types'

import { FloatingPanel, FloatingPanelProps } from './floating-panel/FloatingPanel'

export enum PopoverOpenEventReason {
    TriggerClick = 'TriggerClick',
    TriggerFocus = 'TriggerFocus',
    TriggerBlur = 'TriggerBlur',
    ClickOutside = 'ClickOutside',
    Esc = 'Esc',
}

export interface PopoverOpenEvent {
    isOpen: boolean
    reason: PopoverOpenEventReason
}

interface PopoverContextData {
    isOpen: boolean
    targetElement: HTMLElement | null
    tailElement: HTMLElement | null
    anchor?: MutableRefObject<HTMLElement | null>
    setOpen: (event: PopoverOpenEvent) => void
    setTargetElement: (element: HTMLElement | null) => void
    setTailElement: (element: HTMLElement | null) => void
}

const DEFAULT_CONTEXT_VALUE: PopoverContextData = {
    isOpen: false,
    targetElement: null,
    tailElement: null,
    setOpen: noop,
    setTargetElement: noop,
    setTailElement: noop,
}

const PopoverContext = createContext<PopoverContextData>(DEFAULT_CONTEXT_VALUE)

interface PopoverProps {
    anchor?: MutableRefObject<HTMLElement | null>
    isOpen?: boolean
    onOpenChange?: (event: PopoverOpenEvent) => void
}

export const Popover: React.FunctionComponent<PopoverProps> = props => {
    const { children, anchor, isOpen, onOpenChange = noop } = props

    const [targetElement, setTargetElement] = useState<HTMLElement | null>(null)
    const [tailElement, setTailElement] = useState<HTMLElement | null>(null)

    const [isInternalOpen, setInternalOpen] = useState<boolean>(false)
    const isControlled = isOpen !== undefined
    const isPopoverOpen = isControlled ? isOpen : isInternalOpen
    const setOpen = useCallback<(event: PopoverOpenEvent) => void>(
        event => {
            if (!isControlled) {
                setInternalOpen(event.isOpen)
            }

            onOpenChange(event)
        },
        [isControlled, onOpenChange]
    )

    const context = useMemo(
        () => ({
            isOpen: isPopoverOpen,
            targetElement,
            tailElement,
            anchor,
            setOpen,
            setTargetElement,
            setTailElement,
        }),
        [isPopoverOpen, targetElement, tailElement, anchor, setOpen]
    )

    return <PopoverContext.Provider value={context}>{children}</PopoverContext.Provider>
}

interface PopoverTriggerProps {}

export const PopoverTrigger = forwardRef((props, reference) => {
    const { as: Component = 'button', onClick = noop, ...otherProps } = props
    const { setTargetElement, setOpen, isOpen } = useContext(PopoverContext)

    const callbackReference = useCallbackRef<HTMLButtonElement>(null, setTargetElement)
    const mergedReference = useMergeRefs([reference, callbackReference])

    const handleClick: React.MouseEventHandler<HTMLButtonElement> = event => {
        setOpen({ isOpen: !isOpen, reason: PopoverOpenEventReason.TriggerClick })
        onClick(event)
    }

    return <Component ref={mergedReference} onClick={handleClick} {...otherProps} />
}) as ForwardReferenceComponent<'button', PopoverTriggerProps>

interface PopoverContentProps extends Omit<FloatingPanelProps, 'target' | 'marker'> {
    isOpen?: boolean
    focusLocked?: boolean
}

export const PopoverContent = forwardRef((props, reference) => {
    const {
        isOpen,
        children,
        focusLocked = true,
        as: Component = 'div',
        role = 'dialog',
        'aria-modal': ariaModel = true,
        ...otherProps
    } = props

    const { isOpen: isOpenContext, targetElement, anchor, setOpen } = useContext(PopoverContext)

    const localReference = useRef<HTMLDivElement>(null)
    const mergeReference = useMergeRefs([localReference, reference])

    // Catch any outside click of popover element
    useOnClickOutside(mergeReference, event => {
        if (targetElement?.contains(event.target as Node)) {
            return
        }

        setOpen({ isOpen: false, reason: PopoverOpenEventReason.ClickOutside })
    })

    // Close popover on escape
    useKeyboard({ detectKeys: ['Escape'] }, () => setOpen({ isOpen: false, reason: PopoverOpenEventReason.Esc }))

    if (!isOpenContext && !isOpen) {
        return null
    }

    return (
        <FloatingPanel
            {...otherProps}
            as={Component}
            ref={mergeReference}
            target={anchor?.current ?? targetElement}
            role={role}
            aria-modal={ariaModel}
            className={classNames('dropdown-menu', otherProps.className)}
        >
            {focusLocked ? <FocusLock returnFocus={true}>{children}</FocusLock> : children}
        </FloatingPanel>
    )
}) as ForwardReferenceComponent<'div', PopoverContentProps>
