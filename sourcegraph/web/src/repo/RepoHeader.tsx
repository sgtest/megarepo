import * as H from 'history'
import ChevronRightIcon from 'mdi-react/ChevronRightIcon'
import SettingsIcon from 'mdi-react/SettingsIcon'
import * as React from 'react'
import { ActionsNavItems } from '../../../shared/src/actions/ActionsNavItems'
import { ContributableMenu } from '../../../shared/src/api/protocol'
import { ExtensionsControllerProps } from '../../../shared/src/extensions/controller'
import * as GQL from '../../../shared/src/graphql/schema'
import { PlatformContextProps } from '../../../shared/src/platform/context'
import { ErrorLike, isErrorLike } from '../../../shared/src/util/errors'
import { ActionItem } from '../components/ActionItem'
import { PopoverButton } from '../components/PopoverButton'
import { displayRepoPath, splitPath } from '../components/RepoFileLink'
import { ActionButtonDescriptor } from '../util/contributions'
import { ResolvedRev } from './backend'
import { RepositoriesPopover } from './RepositoriesPopover'

/**
 * Stores the list of RepoHeaderContributions, manages addition/deletion, and ensures they are sorted.
 *
 * It should be instantiated in a private field of the common ancestor component of RepoHeader and all components
 * needing to contribute to RepoHeader.
 */
class RepoHeaderContributionStore {
    constructor(
        /** The common ancestor component's setState method. */
        private setState: (callback: (prevState: RepoHeaderContributionsProps) => RepoHeaderContributionsProps) => void
    ) {}

    private onRepoHeaderContributionAdd(item: RepoHeaderContribution): void {
        if (!item.element) {
            throw new Error(`RepoHeaderContribution has no element`)
        }
        if (typeof item.element.key !== 'string') {
            throw new Error(`RepoHeaderContribution (${item.element.type.toString()}) element must have a string key`)
        }

        this.setState((prevState: RepoHeaderContributionsProps) => ({
            repoHeaderContributions: prevState.repoHeaderContributions
                .filter(({ element }) => element.key !== item.element.key)
                .concat(item)
                .sort(byPriority),
        }))
    }

    private onRepoHeaderContributionRemove(key: string): void {
        this.setState(prevState => ({
            repoHeaderContributions: prevState.repoHeaderContributions.filter(c => c.element.key !== key),
        }))
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
    position: 'nav' | 'left' | 'right'

    /**
     * Controls the relative order of header action items. The items are laid out from highest priority (at the
     * beginning) to lowest priority (at the end). The default is 0.
     */
    priority?: number

    /**
     * The element to display in the RepoHeader. The element *must* have a React key that is a string and is unique
     * among all RepoHeaderContributions. If not, an exception will be thrown.
     */
    element: React.ReactElement<any>
}

/** React props for components that store or display RepoHeaderContributions. */
export interface RepoHeaderContributionsProps {
    /** Contributed items to display in the RepoHeader. */
    repoHeaderContributions: RepoHeaderContribution[]
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
        onRepoHeaderContributionRemove: (key: string) => void
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
}

export interface RepoHeaderActionButton extends ActionButtonDescriptor<RepoHeaderContext> {}

interface Props extends PlatformContextProps, ExtensionsControllerProps {
    /**
     * An array of render functions for action buttons that can be configured *in addition* to action buttons
     * contributed through {@link RepoHeaderContributionsLifecycleProps} and through extensions.
     */
    actionButtons: ReadonlyArray<RepoHeaderActionButton>

    /**
     * The repository that this header is for.
     */
    repo:
        | GQL.IRepository
        | {
              /** The repository's GQL.ID, if it has one.
               */
              id?: GQL.ID

              name: string
              url: string
              enabled: boolean
              viewerCanAdminister: boolean
          }

    /** Information about the revision of the repository. */
    resolvedRev: ResolvedRev | ErrorLike | undefined

    /** The URI-decoded revision (e.g., "my#branch" in "my/repo@my%23branch"). */
    rev?: string

    /**
     * Called in the constructor when the store is constructed. The parent component propagates these lifecycle
     * callbacks to its children for them to add and remove contributions.
     */
    onLifecyclePropsChange: (lifecycleProps: RepoHeaderContributionsLifecycleProps) => void

    location: H.Location
    history: H.History
}

interface State extends RepoHeaderContributionsProps {}

/**
 * The repository header with the breadcrumb, revision switcher, and other items.
 *
 * Other components can contribute items to the repository header using RepoHeaderContribution.
 */
export class RepoHeader extends React.PureComponent<Props, State> {
    public state: State = {
        repoHeaderContributions: [],
    }

    public constructor(props: Props) {
        super(props)
        props.onLifecyclePropsChange(this.repoHeaderContributionStore.props)
    }

    public render(): JSX.Element | null {
        const navActions = this.state.repoHeaderContributions.filter(({ position }) => position === 'nav')
        const leftActions = this.state.repoHeaderContributions.filter(({ position }) => position === 'left')
        const rightActions = this.state.repoHeaderContributions.filter(({ position }) => position === 'right')

        const [repoDir, repoBase] = splitPath(displayRepoPath(this.props.repo.name))
        const context: RepoHeaderContext = {
            repoName: this.props.repo.name,
            encodedRev: this.props.rev,
        }
        return (
            <nav className="repo-header navbar navbar-expand">
                <div className="navbar-nav">
                    <PopoverButton
                        className="repo-header__section-btn repo-header__repo"
                        globalKeyBinding="r"
                        link={
                            this.props.resolvedRev && !isErrorLike(this.props.resolvedRev)
                                ? this.props.resolvedRev.rootTreeURL
                                : this.props.repo.url
                        }
                        popoverElement={
                            <RepositoriesPopover
                                currentRepo={this.props.repo.id}
                                history={this.props.history}
                                location={this.props.location}
                            />
                        }
                        hideOnChange={this.props.repo.name}
                    >
                        {repoDir ? `${repoDir}/` : ''}
                        <span className="repo-header__repo-basename">{repoBase}</span>
                    </PopoverButton>
                    {!this.props.repo.enabled && (
                        <div
                            className="alert alert-danger repo-header__alert"
                            data-tooltip={
                                this.props.repo.viewerCanAdminister
                                    ? 'Only site admins can access disabled repositories. Go to Settings to enable it.'
                                    : 'Ask the site admin to enable this repository to view and search it.'
                            }
                        >
                            Repository disabled
                        </div>
                    )}
                </div>
                {navActions.map((a, i) => (
                    <div className="navbar-nav" key={a.element.key || i}>
                        <ChevronRightIcon className="icon-inline repo-header__icon-chevron" />
                        <div className="repo-header__rev">{a.element}</div>
                    </div>
                ))}
                <ul className="navbar-nav">
                    {leftActions.map((a, i) => (
                        <li className="nav-item" key={a.element.key || i}>
                            {a.element}
                        </li>
                    ))}
                </ul>
                <div className="repo-header__spacer" />
                <ul className="navbar-nav">
                    <ActionsNavItems
                        menu={ContributableMenu.EditorTitle}
                        extensionsController={this.props.extensionsController}
                        platformContext={this.props.platformContext}
                        location={this.props.location}
                    />
                    {this.props.actionButtons.map(
                        ({ condition = () => true, label, tooltip, icon: Icon, to }) =>
                            condition(context) && (
                                <li className="nav-item" key={label}>
                                    <ActionItem to={to(context)} data-tooltip={tooltip}>
                                        {Icon && <Icon className="icon-inline" />}{' '}
                                        <span className="d-none d-lg-inline">{label}</span>
                                    </ActionItem>
                                </li>
                            )
                    )}
                    {rightActions.map((a, i) => (
                        <li className="nav-item" key={a.element.key || i}>
                            {a.element}
                        </li>
                    ))}
                    {this.props.repo.viewerCanAdminister && (
                        <li className="nav-item">
                            <ActionItem to={`/${this.props.repo.name}/-/settings`} data-tooltip="Repository settings">
                                <SettingsIcon className="icon-inline" />{' '}
                                <span className="d-none d-lg-inline">Settings</span>
                            </ActionItem>
                        </li>
                    )}
                </ul>
            </nav>
        )
    }

    private repoHeaderContributionStore = new RepoHeaderContributionStore(stateUpdate => this.setState(stateUpdate))
}
