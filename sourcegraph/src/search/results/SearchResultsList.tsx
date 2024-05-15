import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as H from 'history'
import { upperFirst } from 'lodash'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import FileIcon from 'mdi-react/FileIcon'
import SearchIcon from 'mdi-react/SearchIcon'
import TimerSandIcon from 'mdi-react/TimerSandIcon'
import * as React from 'react'
import { Link } from 'react-router-dom'
import { Subject, Subscription } from 'rxjs'
import { debounceTime, distinctUntilChanged, filter, first, map, skip, skipUntil } from 'rxjs/operators'
import { buildSearchURLQuery, parseSearchURLQuery } from '..'
import * as GQL from '../../backend/graphqlschema'
import { FileMatch } from '../../components/FileMatch'
import { ModalContainer } from '../../components/ModalContainer'
import { VirtualList } from '../../components/VirtualList'
import { OpenHelpPopoverButton } from '../../global/OpenHelpPopoverButton'
import { eventLogger } from '../../tracking/eventLogger'
import { ErrorLike, isErrorLike } from '../../util/errors'
import { RepositoryIcon } from '../../util/icons' // TODO: Switch to mdi icon
import { isDefined } from '../../util/types'
import { SavedQueryCreateForm } from '../saved-queries/SavedQueryCreateForm'
import { CommitSearchResult } from './CommitSearchResult'
import { RepositorySearchResult } from './RepositorySearchResult'
import { SearchResultsInfoBar } from './SearchResultsInfoBar'

const isSearchResults = (val: any): val is GQL.ISearchResults => val && val.__typename === 'SearchResults'

interface SearchResultsListProps {
    isLightTheme: boolean
    location: H.Location
    history: H.History
    user: GQL.IUser | null

    // Result list
    resultsOrError?: GQL.ISearchResults | ErrorLike
    onShowMoreResultsClick: () => void

    // Expand all feature
    allExpanded: boolean
    onExpandAllResultsToggle: () => void

    // Saved queries
    showSavedQueryModal: boolean
    onSavedQueryModalClose: () => void
    onDidCreateSavedQuery: () => void
    onSaveQueryClick: () => void
    didSave: boolean

    onHelpPopoverToggle: () => void
}

interface State {
    resultsShown: number
    visibleItems: Set<number>
    didScrollToItem: boolean
}

export class SearchResultsList extends React.PureComponent<SearchResultsListProps, State> {
    /** Emits when a result was either scrolled into or out of the page */
    private visibleItemChanges = new Subject<{ isVisible: boolean; index: number }>()
    private nextItemVisibilityChange = (isVisible: boolean, index: number) =>
        this.visibleItemChanges.next({ isVisible, index })

    /** Emits with the index of the first visible result on the page */
    private firstVisibleItems = new Subject<number>()

    /** Refrence to the current scrollable list element */
    private scrollableElementRef: HTMLElement | null = null
    private setScrollableElementRef = (ref: HTMLElement | null) => (this.scrollableElementRef = ref)

    /** Emits with the <VirtualList> elements */
    private virtualListContainerElements = new Subject<HTMLElement | null>()
    private nextVirtualListContainerElement = (ref: HTMLElement | null) => this.virtualListContainerElements.next(ref)

    private jumpToTopClicks = new Subject<void>()
    private nextJumpToTopClick = () => this.jumpToTopClicks.next()

    private subscriptions = new Subscription()

    constructor(props: SearchResultsListProps) {
        super(props)

        this.state = {
            resultsShown: this.getCheckpoint() + 15,
            visibleItems: new Set<number>(),
            didScrollToItem: false,
        }

        // Handle items that have become visible
        this.subscriptions.add(
            this.visibleItemChanges
                .pipe(filter(({ isVisible, index }) => isVisible && !this.state.visibleItems.has(index)))
                .subscribe(({ isVisible, index }) => {
                    this.setState(({ visibleItems }) => {
                        visibleItems.add(index)

                        return {
                            visibleItems: new Set(visibleItems),
                        }
                    })
                })
        )

        // Handle items that are no longer visible
        this.subscriptions.add(
            this.visibleItemChanges
                .pipe(filter(({ isVisible, index }) => !isVisible && this.state.visibleItems.has(index)))
                .subscribe(({ index }) => {
                    this.setState(({ visibleItems }) => {
                        visibleItems.delete(index)

                        return {
                            visibleItems: new Set(visibleItems),
                        }
                    })
                })
        )

        /** Emits when the first visible items has changed */
        const firstVisibleItemChanges = this.firstVisibleItems.pipe(
            // No need to update when unchanged
            distinctUntilChanged(),
            // Wait a little so we don't update while scrolling
            debounceTime(250)
        )

        //  Update the `at` query param with the latest first visible item
        this.subscriptions.add(
            firstVisibleItemChanges
                .pipe(
                    // Skip page load
                    skip(1)
                )
                .subscribe(this.setCheckpoint)
        )

        // Remove the "Jump to top" button when the user starts scrolling
        this.subscriptions.add(
            this.visibleItemChanges
                .pipe(
                    // We know the user has scrolled when the first visible item has changed
                    skipUntil(firstVisibleItemChanges),
                    // Ignore items being scrolled out due to result items expanding as they load
                    filter(({ isVisible }) => isVisible),
                    // No need to keep firing this
                    first()
                )
                .subscribe(() =>
                    this.setState({
                        didScrollToItem: false,
                    })
                )
        )

        // Scroll the list to the item specified by the `at` query param
        this.subscriptions.add(
            this.virtualListContainerElements
                .pipe(
                    filter(isDefined),
                    // Only on page load
                    first(),
                    map(container => ({ container, checkpoint: this.getCheckpoint() })),
                    // Don't scroll to the first item
                    filter(({ checkpoint }) => checkpoint > 0)
                )
                .subscribe(({ container, checkpoint }) => {
                    let itemToScrollTo = container.children.item(checkpoint)

                    // Handle edge case where user manually sets the checkpoint to greater than the number of results
                    if (itemToScrollTo === null) {
                        const lastIndex = container.children.length - 1

                        itemToScrollTo = container.children.item(lastIndex)

                        this.setCheckpoint(lastIndex)
                    }

                    // It seems unlikely, but still possbile for 'scrollableElementRef' to be null here.
                    // It might be possible for the 'onRef' callback of 'VirtualList' to be triggered
                    // (which would kick off this pipeline) BEFORE the 'ref' callback for the
                    // 'search-results-list' div is executed (which would cause this conditional to be met).
                    // We'll log the error and gracefully exit for now, but we might need to re-evaluate our strategy
                    // if we see this error in production.
                    //
                    // If this case occurs, the page will not automatically scroll to the list item
                    // on page load.
                    if (this.scrollableElementRef === null) {
                        console.error('scrollableElement ref was null when trying to scroll to a list item')
                        return
                    }

                    const scrollable = this.scrollableElementRef

                    const scrollTo = itemToScrollTo.getBoundingClientRect().top - scrollable.getBoundingClientRect().top

                    scrollable.scrollTop = scrollTo

                    this.setState({ didScrollToItem: true })
                })
        )

        // Scroll to the top when "Jump to top" is clicked
        this.subscriptions.add(
            this.jumpToTopClicks.subscribe(() => {
                // this.scrollableElementRef will never be null here. 'jumpToTopClicks'
                // only emits events when the "Jump to Top" anchor tag is clicked, which can
                // never occur before that element is rendered (the 'ref' callback for
                // 'search-results-list' would have already been called at this point).
                const scrollable = this.scrollableElementRef!

                scrollable.scrollTop = 0
                this.setState({ didScrollToItem: false })
            })
        )
    }

    public componentDidUpdate(): void {
        const lowestIndex = Array.from(this.state.visibleItems).reduce((low, index) => Math.min(index, low), Infinity)

        this.firstVisibleItems.next(lowestIndex)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): React.ReactNode {
        const parsedQuery = parseSearchURLQuery(this.props.location.search)

        return (
            <React.Fragment>
                {this.state.didScrollToItem && (
                    <div className="search-results-list__jump-to-top">
                        Scrolled to result {this.getCheckpoint()} based on URL.&nbsp;
                        <a href="#" onClick={this.nextJumpToTopClick}>
                            Jump to top.
                        </a>
                    </div>
                )}

                <div className="search-results-list" ref={this.setScrollableElementRef}>
                    {/* Saved Queries Form */}
                    {this.props.showSavedQueryModal && (
                        <ModalContainer
                            onClose={this.props.onSavedQueryModalClose}
                            component={
                                <SavedQueryCreateForm
                                    user={this.props.user}
                                    values={{ query: parsedQuery ? parsedQuery.query : '' }}
                                    onDidCancel={this.props.onSavedQueryModalClose}
                                    onDidCreate={this.props.onDidCreateSavedQuery}
                                />
                            }
                        />
                    )}

                    {this.props.resultsOrError === undefined ? (
                        <div className="text-center">
                            <LoadingSpinner className="icon-inline" /> Loading
                        </div>
                    ) : isErrorLike(this.props.resultsOrError) ? (
                        /* GraphQL, network, query syntax error */
                        <div className="alert alert-warning">
                            <AlertCircleIcon className="icon-inline" />
                            {upperFirst(this.props.resultsOrError.message)}
                        </div>
                    ) : (
                        (() => {
                            const results = this.props.resultsOrError
                            return (
                                <>
                                    {/* Info Bar */}
                                    <SearchResultsInfoBar
                                        user={this.props.user}
                                        results={results}
                                        allExpanded={this.props.allExpanded}
                                        didSave={this.props.didSave}
                                        onDidCreateSavedQuery={this.props.onDidCreateSavedQuery}
                                        onExpandAllResultsToggle={this.props.onExpandAllResultsToggle}
                                        onSaveQueryClick={this.props.onSaveQueryClick}
                                        onShowMoreResultsClick={this.props.onShowMoreResultsClick}
                                    />

                                    {/* Results */}
                                    <VirtualList
                                        itemsToShow={this.state.resultsShown}
                                        onShowMoreItems={this.onBottomHit(results.results.length)}
                                        onVisibilityChange={this.nextItemVisibilityChange}
                                        items={results.results
                                            .map((result, i) => this.renderResult(result, i <= 15))
                                            .filter(isDefined)}
                                        containment={this.scrollableElementRef || undefined}
                                        onRef={this.nextVirtualListContainerElement}
                                    />

                                    {/* Show more button */}
                                    {results.limitHit &&
                                        results.results.length === this.state.resultsShown && (
                                            <button
                                                className="btn btn-secondary btn-block"
                                                onClick={this.props.onShowMoreResultsClick}
                                            >
                                                Show more
                                            </button>
                                        )}

                                    {/* Server-provided help message */}
                                    {results.alert ? (
                                        <div className="alert alert-info">
                                            <h3>
                                                <AlertCircleIcon className="icon-inline" /> {results.alert.title}
                                            </h3>
                                            <p>{results.alert.description}</p>
                                            {results.alert.proposedQueries && (
                                                <>
                                                    <h4>Did you mean:</h4>
                                                    <ul className="list-unstyled">
                                                        {results.alert.proposedQueries.map(proposedQuery => (
                                                            <li key={proposedQuery.query}>
                                                                <Link
                                                                    className="btn btn-secondary btn-sm"
                                                                    to={'/search?' + buildSearchURLQuery(proposedQuery)}
                                                                >
                                                                    {proposedQuery.query || proposedQuery.description}
                                                                </Link>
                                                                {proposedQuery.query &&
                                                                    proposedQuery.description &&
                                                                    ` — ${proposedQuery.description}`}
                                                            </li>
                                                        ))}
                                                    </ul>
                                                </>
                                            )}{' '}
                                        </div>
                                    ) : (
                                        results.results.length === 0 &&
                                        (results.timedout.length > 0 ? (
                                            /* No results, but timeout hit */
                                            <div className="alert alert-warning">
                                                <h3>
                                                    <TimerSandIcon className="icon-inline" /> Search timed out
                                                </h3>
                                                {this.renderRecommendations([
                                                    <>
                                                        Try narrowing your query, or specifying a longer "timeout:" in
                                                        your query.
                                                    </>,
                                                    /* If running Sourcegraph Server, give some smart advice */
                                                    ...(!window.context.sourcegraphDotComMode &&
                                                    !window.context.isRunningDataCenter
                                                        ? [
                                                              <>
                                                                  Upgrade to Sourcegraph Data Center for distributed
                                                                  on-the-fly search and near-instant indexed search
                                                              </>,
                                                              window.context.likelyDockerOnMac
                                                                  ? 'Use Docker Machine instead of Docker for Mac for better performance on macOS'
                                                                  : 'Run Sourcegraph on a server with more CPU and memory, or faster disk IO',
                                                          ]
                                                        : []),
                                                ])}
                                            </div>
                                        ) : (
                                            <>
                                                <div className="alert alert-info d-flex">
                                                    <h3 className="m-0">
                                                        <SearchIcon className="icon-inline" /> No results
                                                    </h3>
                                                </div>
                                            </>
                                        ))
                                    )}
                                </>
                            )
                        })()
                    )}

                    <div className="pb-4" />
                    {this.props.resultsOrError !== undefined && (
                        <OpenHelpPopoverButton
                            className="mb-2"
                            onHelpPopoverToggle={this.props.onHelpPopoverToggle}
                            text="Not seeing expected results?"
                        />
                    )}
                </div>
            </React.Fragment>
        )
    }

    /**
     * Renders the given recommendations in a list if multiple, otherwise returns the first one or undefined
     */
    private renderRecommendations(recommendations: React.ReactNode[]): React.ReactNode {
        if (recommendations.length <= 1) {
            return recommendations[0]
        }
        return (
            <>
                <h4>Recommendations:</h4>
                <ul>{recommendations.map((recommendation, i) => <li key={i}>{recommendation}</li>)}</ul>
            </>
        )
    }

    private renderResult(result: GQL.SearchResult, expanded: boolean): JSX.Element | undefined {
        switch (result.__typename) {
            case 'Repository':
                return <RepositorySearchResult key={'repo:' + result.id} result={result} onSelect={this.logEvent} />
            case 'FileMatch':
                return (
                    <FileMatch
                        key={'file:' + result.file.url}
                        icon={result.lineMatches && result.lineMatches.length > 0 ? RepositoryIcon : FileIcon}
                        result={result}
                        onSelect={this.logEvent}
                        expanded={false}
                        showAllMatches={false}
                        isLightTheme={this.props.isLightTheme}
                        allExpanded={this.props.allExpanded}
                    />
                )
            case 'CommitSearchResult':
                return (
                    <CommitSearchResult
                        key={'commit:' + result.commit.id}
                        location={this.props.location}
                        result={result}
                        onSelect={this.logEvent}
                        expanded={expanded}
                        allExpanded={this.props.allExpanded}
                    />
                )
        }
        return undefined
    }

    /** onBottomHit increments the amount of results to be shown when we have scrolled to the bottom of the list. */
    private onBottomHit = (limit: number): (() => void) => () =>
        this.setState(({ resultsShown }) => ({
            resultsShown: Math.min(limit, resultsShown + 10),
        }))

    /**
     * getCheckpoint gets the location from the hash in the URL. It is used to scroll to the result on page load of the given URL.
     */
    private getCheckpoint(): number {
        const at = this.props.location.hash.replace(/^#/, '')

        let checkpoint: number

        if (!at) {
            checkpoint = 0
        } else {
            checkpoint = parseInt(at, 10)
        }

        // If checkpoint is `0`, remove it.
        if (checkpoint === 0) {
            this.setCheckpoint(0) // `setCheckpoint` removes the hash when it is 0
        }

        return checkpoint
    }

    /** setCheckpoint sets the hash in the URL. It will be used to scroll to the result on page load of the given URL. */
    private setCheckpoint = (checkpoint: number): void => {
        if (!isSearchResults(this.props.resultsOrError) || this.props.resultsOrError.limitHit) {
            return
        }

        const { hash, ...loc } = this.props.location

        let newHash = ''
        if (checkpoint > 0) {
            newHash = `#${checkpoint}`
        }

        this.props.history.replace({
            ...loc,
            hash: newHash,
        })
    }

    private logEvent = () => eventLogger.log('SearchResultClicked')
}
