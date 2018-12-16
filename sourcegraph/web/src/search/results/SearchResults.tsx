import * as H from 'history'
import { isEqual } from 'lodash'
import * as React from 'react'
import { concat, Subject, Subscription } from 'rxjs'
import { catchError, distinctUntilChanged, filter, map, startWith, switchMap, tap } from 'rxjs/operators'
import { parseSearchURLQuery } from '..'
import { SearchFiltersContainer } from '../../../../shared/src/actions/SearchFiltersContainer'
import { ExtensionsControllerProps } from '../../../../shared/src/extensions/controller'
import * as GQL from '../../../../shared/src/graphql/schema'
import { PlatformContextProps } from '../../../../shared/src/platform/context'
import { isSettingsValid, SettingsCascadeProps } from '../../../../shared/src/settings/settings'
import { isErrorLike } from '../../../../shared/src/util/errors'
import { PageTitle } from '../../components/PageTitle'
import { fetchHighlightedFileLines } from '../../repo/backend'
import { Settings } from '../../schema/settings.schema'
import { eventLogger } from '../../tracking/eventLogger'
import { search } from '../backend'
import { FilterChip } from '../FilterChip'
import { isSearchResults, submitSearch, toggleSearchFilter } from '../helpers'
import { queryTelemetryData } from '../queryTelemetry'
import { SearchResultsList } from './SearchResultsList'
import { SearchResultsListOld } from './SearchResultsListOld'

const UI_PAGE_SIZE = 75

interface SearchResultsProps extends ExtensionsControllerProps, SettingsCascadeProps, PlatformContextProps {
    authenticatedUser: GQL.IUser | null
    location: H.Location
    history: H.History
    isLightTheme: boolean
    navbarSearchQuery: string
}

interface SearchScope {
    name?: string
    value: string
}
interface SearchResultsState {
    /** The loaded search results, error or undefined while loading */
    resultsOrError?: GQL.ISearchResults
    allExpanded: boolean

    // TODO: Remove when newSearchResultsList is removed
    uiLimit: number

    // Saved Queries
    showSavedQueryModal: boolean
    didSaveQuery: boolean
}

const newRepoFilters = localStorage.getItem('newRepoFilters') !== 'false'
const newSearchResultsList = localStorage.getItem('newSearchResultsList') !== 'false'

export class SearchResults extends React.Component<SearchResultsProps, SearchResultsState> {
    public state: SearchResultsState = {
        didSaveQuery: false,
        showSavedQueryModal: false,
        allExpanded: false,
        uiLimit: UI_PAGE_SIZE,
    }

    /** Emits on componentDidUpdate with the new props */
    private componentUpdates = new Subject<SearchResultsProps>()

    private subscriptions = new Subscription()

    public componentDidMount(): void {
        eventLogger.logViewEvent('SearchResults')

        this.subscriptions.add(
            this.componentUpdates
                .pipe(
                    startWith(this.props),
                    map(props => parseSearchURLQuery(props.location.search)),
                    // Search when a new search query was specified in the URL
                    distinctUntilChanged((a, b) => isEqual(a, b)),
                    filter((query): query is string => !!query),
                    tap(query => {
                        eventLogger.log('SearchResultsQueried', {
                            code_search: { query_data: queryTelemetryData(query) },
                        })
                    }),
                    switchMap(query =>
                        concat(
                            // Reset view state
                            [{ resultsOrError: undefined, didSave: false }],
                            // Do async search request
                            search(query, this.props).pipe(
                                // Log telemetry
                                tap(
                                    results =>
                                        eventLogger.log('SearchResultsFetched', {
                                            code_search: {
                                                // 🚨 PRIVACY: never provide any private data in { code_search: { results } }.
                                                results: {
                                                    results_count: isErrorLike(results) ? 0 : results.results.length,
                                                    any_cloning: isErrorLike(results)
                                                        ? false
                                                        : results.cloning.length > 0,
                                                },
                                            },
                                        }),
                                    error => {
                                        eventLogger.log('SearchResultsFetchFailed', {
                                            code_search: { error_message: error.message },
                                        })
                                        console.error(error)
                                    }
                                ),
                                // Update view with results or error
                                map(results => ({ resultsOrError: results })),
                                catchError(error => [{ resultsOrError: error }])
                            )
                        )
                    )
                )
                .subscribe(newState => this.setState(newState as SearchResultsState), err => console.error(err))
        )
    }

    public componentDidUpdate(prevProps: SearchResultsProps): void {
        this.componentUpdates.next(this.props)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    private showSaveQueryModal = () => {
        this.setState({ showSavedQueryModal: true, didSaveQuery: false })
    }

    private onDidCreateSavedQuery = () => {
        eventLogger.log('SavedQueryCreated')
        this.setState({ showSavedQueryModal: false, didSaveQuery: true })
    }

    private onModalClose = () => {
        eventLogger.log('SavedQueriesToggleCreating', { queries: { creating: false } })
        this.setState({ didSaveQuery: false, showSavedQueryModal: false })
    }

    public render(): JSX.Element | null {
        const query = parseSearchURLQuery(this.props.location.search)
        const filters = this.getFilters()
        const extensionFilters = (
            <SearchFiltersContainer
                // tslint:disable-next-line:jsx-no-lambda
                render={items => (
                    <>
                        {items
                            .filter(item => item.name && item.value)
                            .map((item, i) => (
                                <FilterChip
                                    query={this.props.navbarSearchQuery}
                                    onFilterChosen={this.onDynamicFilterClicked}
                                    key={item.name + item.value}
                                    value={item.value}
                                    name={item.name}
                                />
                            ))}
                    </>
                )}
                empty={null}
                extensionsController={this.props.extensionsController}
            />
        )
        return (
            <div className="search-results">
                <PageTitle key="page-title" title={query} />
                {((isSearchResults(this.state.resultsOrError) && filters.length > 0) || extensionFilters) && (
                    <div className="search-results__filters-bar">
                        Filters:
                        <div className="search-results__filters">
                            {extensionFilters}
                            {filters
                                .filter(filter => filter.value !== '')
                                .map((filter, i) => (
                                    <FilterChip
                                        query={this.props.navbarSearchQuery}
                                        onFilterChosen={this.onDynamicFilterClicked}
                                        key={filter.name + filter.value}
                                        value={filter.value}
                                        name={filter.name}
                                    />
                                ))}
                        </div>
                    </div>
                )}
                {newRepoFilters &&
                    isSearchResults(this.state.resultsOrError) &&
                    this.state.resultsOrError.dynamicFilters.filter(filter => filter.kind === 'repo').length > 0 && (
                        <div className="search-results__filters-bar">
                            Repositories:
                            <div className="search-results__filters">
                                {this.state.resultsOrError.dynamicFilters
                                    .filter(filter => filter.kind === 'repo' && filter.value !== '')
                                    .map((filter, i) => (
                                        <FilterChip
                                            name={filter.label}
                                            query={this.props.navbarSearchQuery}
                                            onFilterChosen={this.onDynamicFilterClicked}
                                            key={filter.value}
                                            value={filter.value}
                                            count={filter.count}
                                            limitHit={filter.limitHit}
                                        />
                                    ))}
                                {this.state.resultsOrError.limitHit &&
                                    !/\brepo:/.test(this.props.navbarSearchQuery) && (
                                        <FilterChip
                                            name="Show more"
                                            query={this.props.navbarSearchQuery}
                                            onFilterChosen={this.showMoreResults}
                                            key={`count:${this.calculateCount()}`}
                                            value={`count:${this.calculateCount()}`}
                                            showMore={true}
                                        />
                                    )}
                            </div>
                        </div>
                    )}
                {newSearchResultsList ? (
                    <SearchResultsList
                        resultsOrError={this.state.resultsOrError}
                        onShowMoreResultsClick={this.showMoreResults}
                        onExpandAllResultsToggle={this.onExpandAllResultsToggle}
                        allExpanded={this.state.allExpanded}
                        showSavedQueryModal={this.state.showSavedQueryModal}
                        onSaveQueryClick={this.showSaveQueryModal}
                        onSavedQueryModalClose={this.onModalClose}
                        onDidCreateSavedQuery={this.onDidCreateSavedQuery}
                        didSave={this.state.didSaveQuery}
                        location={this.props.location}
                        history={this.props.history}
                        authenticatedUser={this.props.authenticatedUser}
                        settingsCascade={this.props.settingsCascade}
                        isLightTheme={this.props.isLightTheme}
                        fetchHighlightedFileLines={fetchHighlightedFileLines}
                    />
                ) : (
                    <SearchResultsListOld
                        resultsOrError={this.state.resultsOrError}
                        onShowMoreResultsClick={this.showMoreResults}
                        onExpandAllResultsToggle={this.onExpandAllResultsToggle}
                        allExpanded={this.state.allExpanded}
                        showSavedQueryModal={this.state.showSavedQueryModal}
                        onSaveQueryClick={this.showSaveQueryModal}
                        onSavedQueryModalClose={this.onModalClose}
                        onDidCreateSavedQuery={this.onDidCreateSavedQuery}
                        didSave={this.state.didSaveQuery}
                        location={this.props.location}
                        authenticatedUser={this.props.authenticatedUser}
                        isLightTheme={this.props.isLightTheme}
                        settingsCascade={this.props.settingsCascade}
                        uiLimit={this.state.uiLimit}
                        fetchHighlightedFileLines={fetchHighlightedFileLines}
                    />
                )}
            </div>
        )
    }

    /** Combines dynamic filters and search scopes into a list de-duplicated by value. */
    private getFilters(): SearchScope[] {
        const filters = new Map<string, SearchScope>()

        if (isSearchResults(this.state.resultsOrError) && this.state.resultsOrError.dynamicFilters) {
            let dynamicFilters = this.state.resultsOrError.dynamicFilters
            if (newRepoFilters) {
                dynamicFilters = this.state.resultsOrError.dynamicFilters.filter(filter => filter.kind !== 'repo')
            }
            for (const d of dynamicFilters) {
                filters.set(d.value, d)
            }
        }
        const scopes =
            (isSettingsValid<Settings>(this.props.settingsCascade) &&
                this.props.settingsCascade.final['search.scopes']) ||
            []
        if (isSearchResults(this.state.resultsOrError) && this.state.resultsOrError.dynamicFilters) {
            for (const scope of scopes) {
                if (!filters.has(scope.value)) {
                    filters.set(scope.value, scope)
                }
            }
        } else {
            for (const scope of scopes) {
                // Check for if filter.value already exists and if so, overwrite with user's configured scope name.
                const existingFilter = filters.get(scope.value)
                // This works because user setting configs are the last to be processed after Global and Org.
                // Thus, user set filters overwrite the equal valued existing filters.
                if (existingFilter) {
                    existingFilter.name = scope.name || scope.value
                }
                filters.set(scope.value, existingFilter || scope)
            }
        }

        return Array.from(filters.values())
    }
    private showMoreResults = () => {
        // Requery with an increased max result count.
        const params = new URLSearchParams(this.props.location.search)
        let query = params.get('q') || ''

        const count = this.calculateCount()
        if (/count:(\d+)/.test(query)) {
            query = query.replace(/count:\d+/g, '').trim() + ` count:${count}`
        } else {
            query = `${query} count:${count}`
        }
        params.set('q', query)
        this.props.history.replace({ search: params.toString() })
    }

    private calculateCount = (): number => {
        // This function can only get called if the results were successfully loaded,
        // so casting is the right thing to do here
        const results = this.state.resultsOrError as GQL.ISearchResults

        const params = new URLSearchParams(this.props.location.search)
        const query = params.get('q') || ''

        if (/count:(\d+)/.test(query)) {
            return Math.max(results.resultCount * 2, 1000)
        }
        return Math.max(results.resultCount * 2 || 0, 1000)
    }

    private onExpandAllResultsToggle = () => {
        this.setState(
            state => ({ allExpanded: !state.allExpanded }),
            () => {
                eventLogger.log(this.state.allExpanded ? 'allResultsExpanded' : 'allResultsCollapsed')
            }
        )
    }

    private onDynamicFilterClicked = (value: string) => {
        eventLogger.log('DynamicFilterClicked', {
            search_filter: { value },
        })
        submitSearch(this.props.history, toggleSearchFilter(this.props.navbarSearchQuery, value), 'filter')
    }
}
