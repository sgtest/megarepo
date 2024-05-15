import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as H from 'history'
import AddIcon from 'mdi-react/AddIcon'
import AutoFixIcon from 'mdi-react/AutoFixIcon'
import HelpCircleOutlineIcon from 'mdi-react/HelpCircleOutlineIcon'
import * as React from 'react'
import { Redirect } from 'react-router'
import { Link } from 'react-router-dom'
import { Subject, Subscription } from 'rxjs'
import { map } from 'rxjs/operators'
import { currentUser } from '../../auth'
import * as GQL from '../../backend/graphqlschema'
import { siteFlags } from '../../site/backend'
import { eventLogger } from '../../tracking/eventLogger'
import { observeSavedQueries } from '../backend'
import { ExampleSearches } from './ExampleSearches'
import { SavedQuery } from './SavedQuery'
import { SavedQueryCreateForm } from './SavedQueryCreateForm'
import { SavedQueryFields } from './SavedQueryForm'

interface Props {
    user: GQL.IUser | null
    location: H.Location
    isLightTheme: boolean
    hideExampleSearches?: boolean
}

interface State {
    savedQueries: GQL.ISavedQuery[]

    /**
     * Whether the saved query creation form is visible.
     */
    isCreating: boolean

    loading: boolean
    error?: Error
    user: GQL.IUser | null

    isViewingExamples: boolean
    exampleQuery: Partial<SavedQueryFields> | null
    disableBuiltInSearches: boolean
}

const EXAMPLE_SEARCHES_CLOSED_KEY = 'example-searches-closed'

export class SavedQueries extends React.Component<Props, State> {
    public state: State = {
        savedQueries: [],
        isCreating: false,
        loading: true,
        user: null,
        isViewingExamples: window.context.sourcegraphDotComMode
            ? false
            : localStorage.getItem(EXAMPLE_SEARCHES_CLOSED_KEY) !== 'true',
        exampleQuery: null,
        disableBuiltInSearches: false,
    }

    private componentUpdates = new Subject<Props>()
    private subscriptions = new Subscription()

    public componentDidMount(): void {
        const isHomepage = this.props.location.pathname === '/search'

        this.subscriptions.add(
            observeSavedQueries()
                .pipe(
                    map(savedQueries => ({
                        savedQueries: savedQueries.filter(query => !isHomepage || query.showOnHomepage).sort((a, b) => {
                            if (a.description < b.description) {
                                return -1
                            }
                            if (a.description === b.description && a.index < b.index) {
                                return -1
                            }
                            return 1
                        }),
                        loading: false,
                    }))
                )
                .subscribe(newState => this.setState(newState as State), err => console.error(err))
        )

        this.subscriptions.add(
            siteFlags
                .pipe(map(({ disableBuiltInSearches }) => disableBuiltInSearches))
                .subscribe(disableBuiltInSearches => {
                    this.setState({
                        // TODO: Remove the need to check sourcegraphDotComMode by adding this to config
                        disableBuiltInSearches: window.context.sourcegraphDotComMode || disableBuiltInSearches,
                    })
                })
        )

        this.subscriptions.add(currentUser.subscribe(user => this.setState({ user })))
    }

    public componentWillReceiveProps(newProps: Props): void {
        this.componentUpdates.next(newProps)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        if (this.state.loading) {
            return <LoadingSpinner />
        }

        const isHomepage = this.props.location.pathname === '/search'
        const isPanelOpen = this.state.isViewingExamples || this.state.isCreating

        // If not logged in, redirect to sign in
        if (!this.state.user && !isHomepage) {
            const newUrl = new URL(window.location.href)
            // Return to the current page after sign up/in.
            newUrl.searchParams.set('returnTo', window.location.href)
            return <Redirect to={'/sign-up' + newUrl.search} />
        }

        return (
            <div className="saved-queries">
                {!isHomepage && (
                    <div>
                        <div className="saved-queries__header">
                            <h3>{!isPanelOpen && 'Saved searches'}</h3>
                            <div className="saved-queries__actions">
                                {!this.state.disableBuiltInSearches && (
                                    <button
                                        className="btn btn-link"
                                        onClick={this.toggleExamples}
                                        disabled={this.state.isViewingExamples}
                                    >
                                        <AutoFixIcon className="icon-inline" /> Discover built-in searches
                                    </button>
                                )}

                                <button
                                    className="btn btn-link"
                                    onClick={this.toggleCreating}
                                    disabled={this.state.isCreating}
                                >
                                    <AddIcon className="icon-inline" /> Add new search
                                </button>

                                <a
                                    href="https://about.sourcegraph.com/docs/search/saved-searches"
                                    onClick={this.onDidClickQueryHelp}
                                    className="btn btn-link"
                                    target="_blank"
                                >
                                    <HelpCircleOutlineIcon className="icon-inline" /> Help
                                </a>
                            </div>
                        </div>
                        {this.state.isCreating && (
                            <SavedQueryCreateForm
                                user={this.props.user}
                                onDidCreate={this.onDidCreateSavedQuery}
                                onDidCancel={this.toggleCreating}
                                values={this.state.exampleQuery || {}}
                            />
                        )}
                    </div>
                )}
                <div>
                    {!this.props.hideExampleSearches &&
                        !this.state.isCreating &&
                        this.state.isViewingExamples && (
                            <ExampleSearches
                                isLightTheme={this.props.isLightTheme}
                                onClose={this.toggleExamples}
                                onExampleSelected={this.onExampleSelected}
                            />
                        )}
                    {!this.state.disableBuiltInSearches &&
                        !this.props.hideExampleSearches &&
                        isPanelOpen && (
                            <div className="saved-queries__header saved-queries__space">
                                <h3>Saved searches</h3>
                            </div>
                        )}
                    {!isHomepage &&
                        this.state.savedQueries.length === 0 && <p>You don't have any saved searches yet.</p>}
                    {this.state.savedQueries.map((savedQuery, i) => (
                        <SavedQuery
                            user={this.props.user}
                            key={`${savedQuery.query}-${i}`}
                            savedQuery={savedQuery}
                            onDidDuplicate={this.onDidDuplicateSavedQuery}
                            isLightTheme={this.props.isLightTheme}
                        />
                    ))}
                </div>
                {this.state.savedQueries.length === 0 &&
                    this.state.user &&
                    isHomepage && (
                        <div className="saved-query">
                            <Link to="/search/searches">
                                <div className={`saved-query__row`}>
                                    <div className="saved-query-row__add-query">
                                        <AddIcon className="icon-inline" /> Add a new search to start monitoring your
                                        code
                                    </div>
                                </div>
                            </Link>
                        </div>
                    )}
            </div>
        )
    }

    private toggleCreating = () => {
        eventLogger.log('SavedQueriesToggleCreating', { queries: { creating: !this.state.isCreating } })
        this.setState(state => ({ isCreating: !state.isCreating, exampleQuery: null, isViewingExamples: false }))
    }

    private toggleExamples = () => {
        eventLogger.log('SavedQueriesToggleExamples', { queries: { viewingExamples: !this.state.isViewingExamples } })

        this.setState(
            state => ({
                isViewingExamples: !state.isViewingExamples,
                exampleQuery: null,
                isCreating: false,
            }),
            () => {
                if (!this.state.isViewingExamples && localStorage.getItem(EXAMPLE_SEARCHES_CLOSED_KEY) !== 'true') {
                    localStorage.setItem(EXAMPLE_SEARCHES_CLOSED_KEY, 'true')
                }
            }
        )
    }

    private onExampleSelected = (query: Partial<SavedQueryFields>) => {
        eventLogger.log('SavedQueryExampleSelected', { queries: { example: query } })
        this.setState({ isViewingExamples: false, isCreating: true, exampleQuery: query })
    }

    private onDidCreateSavedQuery = () => {
        eventLogger.log('SavedQueryCreated')
        this.setState({ isCreating: false, exampleQuery: null })
    }

    private onDidDuplicateSavedQuery = () => {
        eventLogger.log('SavedQueryDuplicated')
    }

    private onDidClickQueryHelp = () => {
        eventLogger.log('SavedQueriesHelpButtonClicked')
    }
}

export class SavedQueriesPage extends SavedQueries {
    public componentDidMount(): void {
        super.componentDidMount()
        eventLogger.logViewEvent('SavedQueries')
    }
}
