import * as React from 'react'
import { Subject, Subscription } from 'rxjs'
import { switchMap } from 'rxjs/operators'
import { ContributionScope } from '../api/client/context/context'
import { getContributedActionItems } from '../contributions/contributions'
import { TelemetryProps } from '../telemetry/telemetryService'
import { ActionItem } from './ActionItem'
import { ActionsState } from './actions'
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

export interface ActionsNavItemsProps extends ActionsProps, ActionNavItemsClassProps, TelemetryProps {
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
export class ActionsNavItems extends React.PureComponent<ActionsNavItemsProps, ActionsState> {
    public state: ActionsState = {}

    private scopeChanges = new Subject<ContributionScope | undefined>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        this.subscriptions.add(
            this.scopeChanges
                .pipe(switchMap(scope => this.props.extensionsController.services.contribution.getContributions(scope)))
                .subscribe(contributions => this.setState({ contributions }))
        )
        this.scopeChanges.next(this.props.scope)
    }

    public componentDidUpdate(prevProps: ActionsProps): void {
        if (prevProps.scope !== this.props.scope) {
            this.scopeChanges.next(this.props.scope)
        }
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | React.ReactFragment | null {
        if (!this.state.contributions) {
            return null // loading
        }

        const actionItems = getContributedActionItems(this.state.contributions, this.props.menu).map((item, i) => (
            <React.Fragment key={item.action.id}>
                {' '}
                <li className={this.props.listItemClass}>
                    <ActionItem
                        key={item.action.id}
                        {...item}
                        {...this.props}
                        variant="actionItem"
                        iconClassName={this.props.actionItemIconClass}
                        className={this.props.actionItemClass}
                        pressedClassName={this.props.actionItemPressedClass}
                    />
                </li>
            </React.Fragment>
        ))
        if (this.props.wrapInList) {
            return actionItems.length > 0 ? <ul className={this.props.listClass}>{actionItems}</ul> : null
        }
        return <>{actionItems}</>
    }
}
