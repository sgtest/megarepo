import React, { useState, useMemo, useEffect } from 'react'

import classNames from 'classnames'
import * as H from 'history'
import { noop } from 'lodash'
import DotsVerticalIcon from 'mdi-react/DotsVerticalIcon'

import { ErrorLike } from '@sourcegraph/common'
import { Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import * as GQL from '@sourcegraph/shared/src/schema'
import { SettingsCascadeOrError } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Menu, MenuItem, MenuList, Position, Icon } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { Breadcrumbs, BreadcrumbsProps } from '../components/Breadcrumbs'
import { ErrorBoundary } from '../components/ErrorBoundary'
import { ActionItemsToggle, ActionItemsToggleProps } from '../extensions/components/ActionItemsBar'
import { ActionButtonDescriptor } from '../util/contributions'
import { useBreakpoint } from '../util/dom'

import { ResolvedRevision } from './backend'
import { RepoHeaderActionDropdownToggle } from './components/RepoHeaderActions'

import styles from './RepoHeader.module.scss'

/**
 * Stores the list of RepoHeaderContributions, manages addition/deletion, and ensures they are sorted.
 *
 * It should be instantiated in a private field of the common ancestor component of RepoHeader and all components
 * needing to contribute to RepoHeader.
 */
class RepoHeaderContributionStore {
    constructor(
        /** The common ancestor component's setState method. */
        private setState: (callback: (previousState: RepoHeaderContribution[]) => RepoHeaderContribution[]) => void
    ) {}

    private onRepoHeaderContributionAdd(item: RepoHeaderContribution): void {
        if (!item.children || typeof item.children !== 'function') {
            throw new Error('RepoHeaderContribution has no child render function')
        }

        this.setState((previousContributions: RepoHeaderContribution[]) =>
            previousContributions
                .filter(({ id }) => id !== item.id)
                .concat(item)
                .sort(byPriority)
        )
    }

    private onRepoHeaderContributionRemove(id: string): void {
        this.setState(previousContributions => previousContributions.filter(contribution => contribution.id !== id))
    }

    /** Props to pass to the owner's children (that need to contribute to RepoHeader). */
    public readonly props: RepoHeaderContributionsLifecycleProps = {
        repoHeaderContributionsLifecycleProps: {
            onRepoHeaderContributionAdd: this.onRepoHeaderContributionAdd.bind(this),
            onRepoHeaderContributionRemove: this.onRepoHeaderContributionRemove.bind(this),
        },
    }
}

function byPriority(a: { priority?: number }, b: { priority?: number }): number {
    return (b.priority || 0) - (a.priority || 0)
}

/**
 * An item that is displayed in the RepoHeader and originates from outside the RepoHeader. The item is typically an
 * icon, button, or link.
 */
export interface RepoHeaderContribution {
    /** The position of this contribution in the RepoHeader. */
    position: 'left' | 'right'

    /**
     * Controls the relative order of header action items. The items are laid out from highest priority (at the
     * beginning) to lowest priority (at the end). The default is 0.
     */
    priority?: number

    id: string

    /**
     * Render function called with RepoHeaderContext.
     * Use `actionType` to determine how to render the component.
     */
    children: (context: RepoHeaderContext) => React.ReactElement
}

/**
 * React props for components that participate in the creation or lifecycle of RepoHeaderContributions.
 */
export interface RepoHeaderContributionsLifecycleProps {
    repoHeaderContributionsLifecycleProps?: {
        /**
         * Called when a new RepoHeader contribution is created (and should be shown in RepoHeader). If another
         * contribution with the same ID already exists, this new one overwrites the existing one.
         */
        onRepoHeaderContributionAdd: (item: RepoHeaderContribution) => void

        /**
         * Called when a new RepoHeader contribution is removed (and should no longer be shown in RepoHeader). The key
         * is the same as that of the contribution's element (when it was added).
         */
        onRepoHeaderContributionRemove: (id: string) => void
    }
}

/**
 * Context passed into action button render functions
 */
export interface RepoHeaderContext {
    /** The current repository name */
    repoName: string
    /** The current URI-decoded revision (e.g., "my#branch" in "my/repo@my%23branch"). */
    encodedRev?: string
    /** `actionType` is 'nav' on lg screens, `dropdown` on smaller screens. */
    actionType: 'nav' | 'dropdown'
}

export interface RepoHeaderActionButton extends ActionButtonDescriptor<RepoHeaderContext> {}

interface Props extends PlatformContextProps, TelemetryProps, BreadcrumbsProps, ActionItemsToggleProps {
    /**
     * An array of render functions for action buttons that can be configured *in addition* to action buttons
     * contributed through {@link RepoHeaderContributionsLifecycleProps} and through extensions.
     */
    actionButtons: readonly RepoHeaderActionButton[]

    /**
     * The repository that this header is for.
     */
    repo:
        | GQL.IRepository
        | {
              /** The repository's ID, if it has one.
               */
              id?: Scalars['ID']

              name: string
              url: string
              viewerCanAdminister: boolean
          }

    /** Information about the revision of the repository. */
    resolvedRev: ResolvedRevision | ErrorLike | undefined

    /** The URI-decoded revision (e.g., "my#branch" in "my/repo@my%23branch"). */
    revision?: string

    /**
     * Called in the constructor when the store is constructed. The parent component propagates these lifecycle
     * callbacks to its children for them to add and remove contributions.
     */
    onLifecyclePropsChange: (lifecycleProps: RepoHeaderContributionsLifecycleProps) => void

    settingsCascade: SettingsCascadeOrError

    authenticatedUser: AuthenticatedUser | null

    location: H.Location
    history: H.History
}

/**
 * The repository header with the breadcrumb, revision switcher, and other items.
 *
 * Other components can contribute items to the repository header using RepoHeaderContribution.
 */
export const RepoHeader: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    onLifecyclePropsChange,
    resolvedRev,
    repo,
    ...props
}) => {
    const [repoHeaderContributions, setRepoHeaderContributions] = useState<RepoHeaderContribution[]>([])
    const repoHeaderContributionStore = useMemo(
        () => new RepoHeaderContributionStore(contributions => setRepoHeaderContributions(contributions)),
        [setRepoHeaderContributions]
    )
    useEffect(() => {
        onLifecyclePropsChange(repoHeaderContributionStore.props)
    }, [onLifecyclePropsChange, repoHeaderContributionStore.props])

    const isLarge = useBreakpoint('lg')

    const context: Omit<RepoHeaderContext, 'actionType'> = useMemo(
        () => ({
            repoName: repo.name,
            encodedRev: props.revision,
        }),
        [repo.name, props.revision]
    )

    const leftActions = useMemo(
        () =>
            repoHeaderContributions
                .filter(({ position }) => position === 'left')
                .map(({ children, ...rest }) => ({ ...rest, element: children({ ...context, actionType: 'nav' }) })),
        [context, repoHeaderContributions]
    )
    const rightActions = useMemo(
        () =>
            repoHeaderContributions
                .filter(({ position }) => position === 'right')
                .map(({ children, ...rest }) => ({
                    ...rest,
                    element: children({ ...context, actionType: isLarge ? 'nav' : 'dropdown' }),
                })),
        [context, repoHeaderContributions, isLarge]
    )

    return (
        <nav data-testid="repo-header" className={classNames('navbar navbar-expand', styles.repoHeader)}>
            <div className="d-flex align-items-center flex-shrink-past-contents">
                {/* Breadcrumb for the nav elements */}
                <Breadcrumbs breadcrumbs={props.breadcrumbs} location={props.location} />
            </div>
            <ul className="navbar-nav">
                {leftActions.map((a, index) => (
                    <li className="nav-item" key={a.id || index}>
                        {a.element}
                    </li>
                ))}
            </ul>
            <div className={styles.spacer} />
            <ErrorBoundary
                location={props.location}
                // To be clear to users that this isn't an error reported by extensions
                // about e.g. the code they're viewing.
                render={error => (
                    <ul className="navbar-nav">
                        <li className={classNames('nav-item', styles.actionListItem)}>
                            <span>Component error: {error.message}</span>
                        </li>
                    </ul>
                )}
            >
                {isLarge ? (
                    <ul className="navbar-nav">
                        {rightActions.map((a, index) => (
                            <li className={classNames('nav-item', styles.actionListItem)} key={a.id || index}>
                                {a.element}
                            </li>
                        ))}
                    </ul>
                ) : (
                    <ul className="navbar-nav">
                        <li className="nav-item">
                            <Menu>
                                <RepoHeaderActionDropdownToggle aria-label="Repository actions">
                                    <Icon role="img" as={DotsVerticalIcon} aria-hidden={true} />
                                </RepoHeaderActionDropdownToggle>
                                <MenuList position={Position.bottomEnd}>
                                    {rightActions.map((a, index) => (
                                        <MenuItem
                                            className="p-0"
                                            key={a.id || index}
                                            onSelect={noop}
                                            onMouseUp={event => event.preventDefault()}
                                        >
                                            {a.element}
                                        </MenuItem>
                                    ))}
                                </MenuList>
                            </Menu>
                        </li>
                    </ul>
                )}
                <ul className="navbar-nav">
                    <ActionItemsToggle
                        useActionItemsToggle={props.useActionItemsToggle}
                        extensionsController={props.extensionsController}
                    />
                </ul>
            </ErrorBoundary>
        </nav>
    )
}
