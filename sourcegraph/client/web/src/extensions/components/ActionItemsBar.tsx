import React, { useCallback, useEffect, useMemo, useState } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import { head, last } from 'lodash'
import ChevronDoubleUpIcon from 'mdi-react/ChevronDoubleUpIcon'
import MenuDownIcon from 'mdi-react/MenuDownIcon'
import MenuUpIcon from 'mdi-react/MenuUpIcon'
import PlusIcon from 'mdi-react/PlusIcon'
import PuzzleOutlineIcon from 'mdi-react/PuzzleOutlineIcon'
import { BehaviorSubject } from 'rxjs'
import { distinctUntilChanged, map } from 'rxjs/operators'
import { focusable, FocusableElement } from 'tabbable'
import { Key } from 'ts-key-enum'

import { ContributableMenu } from '@sourcegraph/client-api'
import { LocalStorageSubject } from '@sourcegraph/common'
import { ActionItem } from '@sourcegraph/shared/src/actions/ActionItem'
import { ActionsContainer } from '@sourcegraph/shared/src/actions/ActionsContainer'
import { haveInitialExtensionsLoaded } from '@sourcegraph/shared/src/api/features'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Button, LoadingSpinner, useObservable, Link, ButtonLink, Icon } from '@sourcegraph/wildcard'

import { ErrorBoundary } from '../../components/ErrorBoundary'
import { useCarousel } from '../../components/useCarousel'

import styles from './ActionItemsBar.module.scss'

const scrollButtonClassName = styles.scroll

function getIconClassName(index: number): string | undefined {
    return (styles as Record<string, string>)[`icon${index % 5}`]
}

function arrowable(element: HTMLElement): FocusableElement[] {
    return focusable(element).filter(
        elm => !elm.classList.contains('disabled') && !elm.classList.contains(scrollButtonClassName)
    )
}

export function useWebActionItems(): Pick<ActionItemsBarProps, 'useActionItemsBar'> &
    Pick<ActionItemsToggleProps, 'useActionItemsToggle'> {
    const toggles = useMemo(() => new LocalStorageSubject('action-items-bar-expanded', true), [])

    const [toggleReference, setToggleReference] = useState<HTMLElement | null>(null)
    const nextToggleReference = useCallback((toggle: HTMLElement) => {
        setToggleReference(toggle)
    }, [])

    const [barReference, setBarReference] = useState<HTMLElement | null>(null)
    const nextBarReference = useCallback((bar: HTMLElement) => {
        setBarReference(bar)
    }, [])

    // Set up keyboard navigation for distant toggle and bar. Remove previous event
    // listeners whenever references change.
    useEffect(() => {
        function onKeyDownToggle(event: KeyboardEvent): void {
            if (event.key === Key.ArrowDown && barReference) {
                const firstBarArrowable = head(arrowable(barReference))
                if (firstBarArrowable) {
                    firstBarArrowable.focus()
                    event.preventDefault()
                }
            }

            if (event.key === Key.ArrowUp && barReference) {
                const lastBarArrowable = last(arrowable(barReference))
                if (lastBarArrowable) {
                    lastBarArrowable.focus()
                    event.preventDefault()
                }
            }
        }

        function onKeyDownBar(event: KeyboardEvent): void {
            if (event.target instanceof HTMLElement && toggleReference && barReference) {
                const arrowableChildren = arrowable(barReference)
                const indexOfTarget = arrowableChildren.indexOf(event.target)

                if (event.key === Key.ArrowDown) {
                    // If this is the last arrowable element, go back to the toggle
                    if (indexOfTarget === arrowableChildren.length - 1) {
                        toggleReference.focus()
                        event.preventDefault()
                        return
                    }

                    const itemToFocus = arrowableChildren[indexOfTarget + 1]
                    if (itemToFocus instanceof HTMLElement) {
                        itemToFocus.focus()
                        event.preventDefault()
                        return
                    }
                }

                if (event.key === Key.ArrowUp) {
                    // If this is the first arrowable element, go back to the toggle
                    if (indexOfTarget === 0) {
                        toggleReference.focus()
                        event.preventDefault()
                        return
                    }

                    const itemToFocus = arrowableChildren[indexOfTarget - 1]
                    if (itemToFocus instanceof HTMLElement) {
                        itemToFocus.focus()
                        event.preventDefault()
                        return
                    }
                }
            }
        }

        toggleReference?.addEventListener('keydown', onKeyDownToggle)
        barReference?.addEventListener('keydown', onKeyDownBar)

        return () => {
            toggleReference?.removeEventListener('keydown', onKeyDownToggle)
            toggleReference?.removeEventListener('keydown', onKeyDownBar)
        }
    }, [toggleReference, barReference])

    const barsReferenceCounts = useMemo(() => new BehaviorSubject(0), [])

    const useActionItemsBar = useCallback(() => {
        // `useActionItemsBar` will be used as a hook
        // eslint-disable-next-line react-hooks/rules-of-hooks
        const isOpen = useObservable(toggles)

        // Let the toggle know it's on the page
        // eslint-disable-next-line react-hooks/rules-of-hooks
        useEffect(() => {
            // Use reference counter so that effect order doesn't matter
            barsReferenceCounts.next(barsReferenceCounts.value + 1)

            return () => barsReferenceCounts.next(barsReferenceCounts.value - 1)
        }, [])

        return { isOpen, barReference: nextBarReference }
    }, [toggles, nextBarReference, barsReferenceCounts])

    const useActionItemsToggle = useCallback(() => {
        // `useActionItemsToggle` will be used as a hook
        // eslint-disable-next-line react-hooks/rules-of-hooks
        const isOpen = useObservable(toggles)

        // eslint-disable-next-line react-hooks/rules-of-hooks
        const toggle = useCallback(() => toggles.next(!isOpen), [isOpen])

        // Only show the action items toggle when the <ActionItemsBar> component is on the page
        // eslint-disable-next-line react-hooks/rules-of-hooks
        const barInPage = !!useObservable(
            // eslint-disable-next-line react-hooks/rules-of-hooks
            useMemo(
                () =>
                    barsReferenceCounts.pipe(
                        map(count => count > 0),
                        distinctUntilChanged()
                    ),
                []
            )
        )

        return { isOpen, toggle, barInPage, toggleReference: nextToggleReference }
    }, [toggles, nextToggleReference, barsReferenceCounts])

    return {
        useActionItemsBar,
        useActionItemsToggle,
    }
}

export interface ActionItemsBarProps extends ExtensionsControllerProps, TelemetryProps, PlatformContextProps {
    useActionItemsBar: () => { isOpen: boolean | undefined; barReference: React.RefCallback<HTMLElement> }
    location: H.Location
}

const actionItemClassName = classNames(
    'd-flex justify-content-center align-items-center text-decoration-none',
    styles.action
)

/**
 * TODO: description
 */
export const ActionItemsBar = React.memo<ActionItemsBarProps>(function ActionItemsBar(props) {
    const { isOpen, barReference } = props.useActionItemsBar()

    const {
        carouselReference,
        canScrollNegative,
        canScrollPositive,
        onNegativeClicked,
        onPositiveClicked,
    } = useCarousel({ direction: 'topToBottom' })

    const haveExtensionsLoaded = useObservable(
        useMemo(() => haveInitialExtensionsLoaded(props.extensionsController.extHostAPI), [props.extensionsController])
    )

    if (!isOpen) {
        return <div className={styles.barCollapsed} />
    }

    return (
        <div className={classNames('p-0 mr-2 position-relative d-flex flex-column', styles.bar)} ref={barReference}>
            {/* To be clear to users that this isn't an error reported by extensions about e.g. the code they're viewing. */}
            <ErrorBoundary location={props.location} render={error => <span>Component error: {error.message}</span>}>
                <ActionItemsDivider />
                {canScrollNegative && (
                    <Button
                        className={classNames('p-0 border-0', styles.scroll, styles.listItem)}
                        onClick={onNegativeClicked}
                        tabIndex={-1}
                        variant="link"
                    >
                        <Icon as={MenuUpIcon} />
                    </Button>
                )}
                <ActionsContainer
                    menu={ContributableMenu.EditorTitle}
                    returnInactiveMenuItems={true}
                    extensionsController={props.extensionsController}
                    empty={null}
                    location={props.location}
                    platformContext={props.platformContext}
                    telemetryService={props.telemetryService}
                >
                    {items => (
                        <ul className={classNames('list-unstyled m-0', styles.list)} ref={carouselReference}>
                            {items.map((item, index) => {
                                const hasIconURL = !!item.action.actionItem?.iconURL
                                const className = classNames(
                                    actionItemClassName,
                                    !hasIconURL && classNames(styles.actionNoIcon, getIconClassName(index), 'text-sm')
                                )
                                const inactiveClassName = classNames(
                                    styles.actionInactive,
                                    !hasIconURL && styles.actionNoIconInactive
                                )
                                const listItemClassName = classNames(
                                    styles.listItem,
                                    index !== items.length - 1 && 'mb-1'
                                )

                                const dataContent = !hasIconURL ? item.action.category?.slice(0, 1) : undefined

                                return (
                                    <li key={item.action.id} className={listItemClassName}>
                                        <ActionItem
                                            {...props}
                                            {...item}
                                            className={className}
                                            dataContent={dataContent}
                                            variant="actionItem"
                                            iconClassName={styles.icon}
                                            pressedClassName={styles.actionPressed}
                                            inactiveClassName={inactiveClassName}
                                            hideLabel={true}
                                            tabIndex={-1}
                                            hideExternalLinkIcon={true}
                                            disabledDuringExecution={true}
                                        />
                                    </li>
                                )
                            })}
                        </ul>
                    )}
                </ActionsContainer>
                {canScrollPositive && (
                    <Button
                        className={classNames('p-0 border-0', styles.scroll, styles.listItem)}
                        onClick={onPositiveClicked}
                        tabIndex={-1}
                        variant="link"
                    >
                        <Icon as={MenuDownIcon} />
                    </Button>
                )}
                {haveExtensionsLoaded && <ActionItemsDivider />}
                <ul className="list-unstyled m-0">
                    <li className={styles.listItem}>
                        <Link
                            to="/extensions"
                            className={classNames(styles.listItem, styles.auxIcon, actionItemClassName)}
                            data-tooltip="Add extensions"
                            aria-label="Add extensions"
                        >
                            <Icon as={PlusIcon} />
                        </Link>
                    </li>
                </ul>
            </ErrorBoundary>
        </div>
    )
})

export interface ActionItemsToggleProps extends ExtensionsControllerProps<'extHostAPI'> {
    useActionItemsToggle: () => {
        isOpen: boolean | undefined
        toggle: () => void
        toggleReference: React.RefCallback<HTMLElement>
        barInPage: boolean
    }
    className?: string
}

export const ActionItemsToggle: React.FunctionComponent<ActionItemsToggleProps> = ({
    useActionItemsToggle,
    extensionsController,
    className,
}) => {
    const { isOpen, toggle, toggleReference, barInPage } = useActionItemsToggle()

    const haveExtensionsLoaded = useObservable(
        useMemo(() => haveInitialExtensionsLoaded(extensionsController.extHostAPI), [extensionsController])
    )

    return barInPage ? (
        <>
            <li className={styles.dividerVertical} />
            <li className={classNames('nav-item mr-2', className)}>
                <div className={classNames(styles.toggleContainer, isOpen && styles.toggleContainerOpen)}>
                    <ButtonLink
                        data-tooltip={`${isOpen ? 'Close' : 'Open'} extensions panel`}
                        className={classNames(actionItemClassName, styles.auxIcon, styles.actionToggle)}
                        onSelect={toggle}
                        ref={toggleReference}
                    >
                        {!haveExtensionsLoaded ? (
                            <LoadingSpinner />
                        ) : isOpen ? (
                            <Icon data-testid="action-items-toggle-open" as={ChevronDoubleUpIcon} />
                        ) : (
                            <Icon as={PuzzleOutlineIcon} />
                        )}
                    </ButtonLink>
                </div>
            </li>
        </>
    ) : null
}

const ActionItemsDivider: React.FunctionComponent<{ className?: string }> = ({ className }) => (
    <div className={classNames('position-relative rounded-sm d-flex', styles.dividerHorizontal, className)} />
)
