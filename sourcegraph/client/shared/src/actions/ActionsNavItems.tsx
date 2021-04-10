import classNames from 'classnames'
import React, { useMemo, useRef } from 'react'
import { combineLatest, from, ReplaySubject } from 'rxjs'
import { switchMap } from 'rxjs/operators'
import { useDeepCompareEffectNoCheck } from 'use-deep-compare-effect'

import { wrapRemoteObservable } from '../api/client/api/common'
import { Context, ContributionScope } from '../api/extension/api/context/context'
import { getContributedActionItems } from '../contributions/contributions'
import { TelemetryProps } from '../telemetry/telemetryService'
import { useObservable } from '../util/useObservable'

import { ActionItem, ActionItemProps } from './ActionItem'
import { ActionsProps } from './ActionsContainer'

export interface ActionNavItemsClassProps {
    /**
     * CSS class name for one action item (`<button>` or `<a>`)
     */
    actionItemClass?: string

    /**
     * Additional CSS class name when the action item is a toogle in its enabled state.
     */
    actionItemPressedClass?: string

    actionItemIconClass?: string

    /**
     * CSS class name for each `<li>` element wrapping the action item.
     */
    listItemClass?: string
}

export interface ActionsNavItemsProps
    extends ActionsProps,
        ActionNavItemsClassProps,
        TelemetryProps,
        Pick<ActionItemProps, 'showLoadingSpinnerDuringExecution'> {
    /**
     * If true, it renders a `<ul className="nav">...</ul>` around the items. If there are no items, it renders `null`.
     *
     * If falsey (the default behavior), it emits a fragment of just the `<li>`s.
     */
    wrapInList?: boolean
    /**
     * Only applied if `wrapInList` is `true`
     */

    listClass?: string
}

/**
 * Renders the actions as a fragment of <li class="nav-item"> elements, for use in a Bootstrap <ul
 * class="nav"> or <ul class="navbar-nav">.
 */
export const ActionsNavItems: React.FunctionComponent<ActionsNavItemsProps> = props => {
    const { scope, extraContext, extensionsController, menu, wrapInList } = props

    const scopeChanges = useMemo(() => new ReplaySubject<ContributionScope>(1), [])
    useDeepCompareEffectNoCheck(() => {
        scopeChanges.next(scope)
    }, [scope])

    const extraContextChanges = useMemo(() => new ReplaySubject<Context<unknown>>(1), [])
    useDeepCompareEffectNoCheck(() => {
        extraContextChanges.next(extraContext)
    }, [extraContext])

    const contributions = useObservable(
        useMemo(
            () =>
                combineLatest([scopeChanges, extraContextChanges, from(extensionsController.extHostAPI)]).pipe(
                    switchMap(([scope, extraContext, extensionHostAPI]) =>
                        wrapRemoteObservable(extensionHostAPI.getContributions({ scope, extraContext }))
                    )
                ),
            [scopeChanges, extraContextChanges, extensionsController]
        )
    )

    const actionItems = useRef<JSX.Element[] | null>(null)

    if (!contributions) {
        // Show last known list while loading, or empty if nothing has been loaded yet
        return <>{actionItems.current}</>
    }

    actionItems.current = getContributedActionItems(contributions, menu).map(item => (
        <React.Fragment key={item.action.id}>
            {' '}
            <li className={props.listItemClass}>
                <ActionItem
                    key={item.action.id}
                    {...item}
                    {...props}
                    variant="actionItem"
                    iconClassName={props.actionItemIconClass}
                    className={classNames('actions-nav-items__action-item', props.actionItemClass)}
                    pressedClassName={props.actionItemPressedClass}
                />
            </li>
        </React.Fragment>
    ))

    if (wrapInList) {
        return actionItems.current.length > 0 ? <ul className={props.listClass}>{actionItems.current}</ul> : null
    }
    return <>{actionItems.current}</>
}
