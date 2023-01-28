import React, { MouseEvent, useMemo, useState, useCallback, useLayoutEffect } from 'react'

import { mdiInformationOutline } from '@mdi/js'
import classnames from 'classnames'

import { shortcutDisplayName } from '@sourcegraph/shared/src/keyboardShortcuts'
import { Icon, useWindowSize } from '@sourcegraph/wildcard'

import { Action, Group, Option } from './suggestionsExtension'

import styles from './Suggestions.module.scss'

function getActionName(action: Action): string {
    switch (action.type) {
        case 'completion':
            return action.name ?? 'Add'
        case 'goto':
            return action.name ?? 'Go to'
        case 'command':
            return action.name ?? 'Run'
    }
}

interface SuggesionsProps {
    id: string
    results: Group[]
    activeRowIndex: number
    open?: boolean
    onSelect(option: Option): void
}

export const Suggestions: React.FunctionComponent<SuggesionsProps> = ({
    id,
    results,
    activeRowIndex,
    onSelect,
    open = false,
}) => {
    const [container, setContainer] = useState<HTMLDivElement | null>(null)

    const handleSelection = useCallback(
        (event: MouseEvent) => {
            const match = (event.target as HTMLElement).closest('li[role="row"]')?.id.match(/\d+x\d+/)
            if (match) {
                // Extracts the group and row index from the elements ID to pass
                // the right option value to the callback.
                const [group, option] = match[0].split('x')
                onSelect(results[+group].options[+option])
            }
        },
        [onSelect, results]
    )

    const { height: windowHeight } = useWindowSize()
    const maxHeight = useMemo(
        // This is using an arbitrary 20px "margin" between the suggestions box
        // and the window border
        () => (container ? `${windowHeight - container.getBoundingClientRect().top - 20}px` : 'auto'),
        // Recompute height when suggestions change
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [container, windowHeight, results]
    )
    const flattenedRows = useMemo(() => results.flatMap(group => group.options), [results])
    const focusedItem = flattenedRows[activeRowIndex]
    const show = open && results.length > 0

    useLayoutEffect(() => {
        if (container) {
            container.querySelector('[aria-selected="true"]')?.scrollIntoView(false)
        }
    }, [container, focusedItem])

    if (!show) {
        return null
    }

    return (
        <div
            ref={setContainer}
            id={id}
            className={styles.container}
            // eslint-disable-next-line react/forbid-dom-props
            style={{ maxHeight }}
        >
            <div className={styles.suggestions} role="grid" onMouseDown={handleSelection} tabIndex={-1}>
                {results.map((group, groupIndex) =>
                    group.options.length > 0 ? (
                        <ul role="rowgroup" key={group.title} aria-labelledby={`${id}-${groupIndex}-label`}>
                            <li id={`${id}-${groupIndex}-label`} role="presentation">
                                {group.title}
                            </li>
                            {group.options.map((option, rowIndex) => (
                                <li
                                    role="row"
                                    key={rowIndex}
                                    id={`${id}-${groupIndex}x${rowIndex}`}
                                    aria-selected={focusedItem === option}
                                >
                                    {option.icon && (
                                        <div className="pr-1">
                                            <Icon className={styles.icon} svgPath={option.icon} aria-hidden="true" />
                                        </div>
                                    )}
                                    <div role="gridcell">
                                        {option.render ? (
                                            option.render(option)
                                        ) : option.matches ? (
                                            <HighlightedLabel label={option.label} matches={option.matches} />
                                        ) : (
                                            option.label
                                        )}
                                    </div>
                                    {option.description && (
                                        <div role="gridcell" className={styles.description}>
                                            {option.description}
                                        </div>
                                    )}
                                    <div className={styles.note}>
                                        <div role="gridcell">{getActionName(option.action)}</div>
                                        {option.alternativeAction && (
                                            <div role="gridcell">{getActionName(option.alternativeAction)}</div>
                                        )}
                                    </div>
                                </li>
                            ))}
                        </ul>
                    ) : null
                )}
            </div>
            {focusedItem && <Footer option={focusedItem} />}
        </div>
    )
}

const Footer: React.FunctionComponent<{ option: Option }> = ({ option }) => (
    <div className={classnames(styles.footer, 'd-flex align-items-center justify-content-between')}>
        <span>
            {option.info?.(option)}
            {!option.info && (
                <>
                    <ActionInfo action={option.action} shortcut="Return" />{' '}
                    {option.alternativeAction && (
                        <ActionInfo action={option.alternativeAction} shortcut="Shift+Return" />
                    )}
                </>
            )}
        </span>
        <Icon className={styles.icon} svgPath={mdiInformationOutline} aria-hidden="true" />
    </div>
)

const ActionInfo: React.FunctionComponent<{ action: Action; shortcut: string }> = ({ action, shortcut }) => {
    const displayName = shortcutDisplayName(shortcut)
    switch (action.type) {
        case 'completion':
            return (
                <>
                    Press <kbd>{displayName}</kbd> to <strong>add</strong> to your query.
                </>
            )
        case 'goto':
            return (
                <>
                    Press <kbd>{displayName}</kbd> to <strong>go to</strong> the suggestion.
                </>
            )
        case 'command':
            return (
                <>
                    Press <kbd>{displayName}</kbd> to <strong>execute</strong> the command.
                </>
            )
    }
}

export const HighlightedLabel: React.FunctionComponent<{ label: string; matches: Set<number> }> = ({
    label,
    matches,
}) => (
    <>
        {[...label].map((char, index) =>
            matches.has(index) ? (
                <span key={index} className={styles.match}>
                    {char}
                </span>
            ) : (
                char
            )
        )}
    </>
)
