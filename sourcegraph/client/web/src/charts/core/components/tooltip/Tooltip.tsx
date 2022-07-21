import React, { FunctionComponent, HTMLAttributes, LiHTMLAttributes, useEffect, useLayoutEffect, useState } from 'react'

import classNames from 'classnames'

import { PopoverContent, Position, createRectangle } from '@sourcegraph/wildcard'

import styles from './Tooltip.module.scss'

const TOOLTIP_PADDING = createRectangle(0, 0, 10, 10)

interface TooltipProps {
    containerElement: SVGSVGElement
    activeElement?: HTMLElement
}

interface TooltipPosition {
    target: HTMLElement | null
    x: number
    y: number
}

export const Tooltip: React.FunctionComponent<React.PropsWithChildren<TooltipProps>> = props => {
    const { containerElement, activeElement = null, children } = props

    const [{ target, ...pinPoint }, setVirtualElement] = useState<TooltipPosition>({
        target: activeElement,
        x: 0,
        y: 0,
    })

    useLayoutEffect(() => {
        if (activeElement) {
            setVirtualElement(state => ({ ...state, target: activeElement }))
        }
    }, [activeElement])

    useEffect(() => {
        function handleMove(event: PointerEvent): void {
            setVirtualElement({
                target: null,
                x: event.clientX,
                y: event.clientY,
            })
        }

        containerElement.addEventListener('pointermove', handleMove)

        return () => {
            containerElement.removeEventListener('pointermove', handleMove)
        }
    }, [containerElement])

    return (
        <PopoverContent
            isOpen={true}
            pin={!target ? pinPoint : null}
            targetElement={target}
            autoFocus={false}
            focusLocked={false}
            targetPadding={TOOLTIP_PADDING}
            position={Position.rightStart}
            className={styles.tooltip}
            tabIndex={-1}
        >
            {children}
        </PopoverContent>
    )
}

interface TooltipListProps extends HTMLAttributes<HTMLUListElement> {}

export const TooltipList: FunctionComponent<TooltipListProps> = props => (
    <ul {...props} className={classNames(styles.tooltipList, props.className)} />
)

export const TooltipListBlankItem: FunctionComponent<LiHTMLAttributes<HTMLLIElement>> = props => (
    <li {...props} className={classNames(styles.item, props.className)} />
)

interface TooltipListItemProps extends LiHTMLAttributes<HTMLLIElement> {
    isActive: boolean
    name: string
    value: number | string
    color: string
    stackedValue?: number | string | null
}

export const TooltipListItem: FunctionComponent<TooltipListItemProps> = props => {
    const { name, value, stackedValue, color, className, isActive, ...attributes } = props

    /* eslint-disable react/forbid-dom-props */
    return (
        <TooltipListBlankItem {...attributes} className={classNames(className, { [styles.itemActive]: isActive })}>
            <div style={{ backgroundColor: color }} className={styles.mark} />

            <span className={styles.legendText}>{name}</span>

            {stackedValue && (
                <span className={styles.legendStackedValue}>
                    {stackedValue}
                    {'\u00A0—\u00A0'}
                </span>
            )}

            <span>{value}</span>
        </TooltipListBlankItem>
    )
}
