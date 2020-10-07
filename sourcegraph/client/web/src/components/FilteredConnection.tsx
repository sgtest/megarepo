import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import * as H from 'history'
import { uniq } from 'lodash'
import * as React from 'react'
import { combineLatest, merge, Observable, of, Subject, Subscription } from 'rxjs'
import {
    catchError,
    debounceTime,
    delay,
    distinctUntilChanged,
    filter,
    map,
    skip,
    startWith,
    switchMap,
    takeUntil,
    tap,
    scan,
    share,
} from 'rxjs/operators'
import * as GQL from '../../../shared/src/graphql/schema'
import { asError, ErrorLike, isErrorLike } from '../../../shared/src/util/errors'
import { pluralize } from '../../../shared/src/util/strings'
import { Form } from './Form'
import { RadioButtons } from './RadioButtons'
import { ErrorMessage } from './alerts'
import { hasProperty } from '../../../shared/src/util/types'

/** Checks if the passed value satisfies the GraphQL Node interface */
const hasID = (value: unknown): value is { id: GQL.ID } =>
    typeof value === 'object' && value !== null && hasProperty('id')(value) && typeof value.id === 'string'

interface FilterProps {
    /** All filters. */
    filters: FilteredConnectionFilter[]

    /** Called when a filter is selected. */
    onDidSelectFilter: (filter: FilteredConnectionFilter) => void

    /** The ID of the active filter. */
    value: string
}

interface FilterState {}

class FilteredConnectionFilterControl extends React.PureComponent<FilterProps, FilterState> {
    public render(): React.ReactFragment {
        return (
            <div className="filtered-connection-filter-control">
                <RadioButtons nodes={this.props.filters} selected={this.props.value} onChange={this.onChange} />
                {this.props.children}
            </div>
        )
    }

    private onChange: React.ChangeEventHandler<HTMLInputElement> = event => {
        const id = event.currentTarget.value
        const filter = this.props.filters.find(filter => filter.id === id)!
        this.props.onDidSelectFilter(filter)
    }
}

/**
 * Fields that belong in ConnectionPropsCommon and that don't depend on the type parameters. These are the fields
 * that are most likely to be needed by callers, and it's simpler for them if they are in a parameter-less type.
 */
interface ConnectionDisplayProps {
    /** list HTML element type. Default is <ul>. */
    listComponent?: 'ul' | 'table' | 'div'

    /** CSS class name for the list element (<ul>, <table>, or <div>). */
    listClassName?: string

    /** CSS class name for the "Show more" button. */
    showMoreClassName?: string

    /** The English noun (in singular form) describing what this connection contains. */
    noun: string

    /** The English noun (in plural form) describing what this connection contains. */
    pluralNoun: string

    /** Do not show a "Show more" button. */
    noShowMore?: boolean

    /** Do not show a count summary if all nodes are visible in the list's first page. */
    noSummaryIfAllNodesVisible?: boolean

    /** The component displayed when the list of nodes is empty. */
    emptyElement?: JSX.Element

    /** The component displayed when all nodes have been fetched. */
    totalCountSummaryComponent?: React.ComponentType<{ totalCount: number }>
    /**
     * Set to true when the GraphQL response is expected to emit an `PageInfo.endCursor` value when
     * there is a subsequent page of results. This will request the next page of results and append
     * them onto the existing list of results instead of requesting twice as many results and
     * replacing the existing results.
     */
    cursorPaging?: boolean
}

/**
 * Props for the FilteredConnection component's result nodes and associated summary/pagination controls.
 *
 * @template N The node type of the GraphQL connection, such as GQL.IRepository (if the connection is GQL.IRepositoryConnection)
 * @template NP Props passed to `nodeComponent` in addition to `{ node: N }`
 */
interface ConnectionPropsCommon<N, NP = {}> extends ConnectionDisplayProps {
    /** Header row to appear above all nodes. */
    headComponent?: React.ComponentType<{ nodes: N[]; totalCount?: number | null }>

    /** Footer row to appear below all nodes. */
    footComponent?: React.ComponentType<{ nodes: N[] }>

    /** The component type to use to display each node. */
    nodeComponent: React.ComponentType<{ node: N } & NP>

    /** Props to pass to each nodeComponent in addition to `{ node: N }`. */
    nodeComponentProps?: NP

    /** An element rendered as a sibling of the filters. */
    additionalFilterElement?: React.ReactElement
}

/** State related to the ConnectionNodes component. */
interface ConnectionStateCommon {
    query: string
    first: number

    connectionQuery?: string

    /** The `PageInfo.endCursor` value from the previous request. */
    after?: string

    /**
     * The number of results that were visible from previous requests. The initial request of
     * a result set will load `visible` items, then will request `first` items on each subsequent
     * request. This has the effect of loading the correct number of visible results when a URL
     * is copied during pagination. This value is only useful with cursor-based paging.
     */
    visible?: number

    /**
     * Whether the connection is loading. It is not equivalent to connection === undefined because we preserve the
     * old data for ~250msec while loading to reduce jitter.
     */
    loading: boolean
}

interface ConnectionNodesProps<C extends Connection<N>, N, NP = {}>
    extends ConnectionPropsCommon<N, NP>,
        ConnectionStateCommon {
    /** The fetched connection data or an error (if an error occurred). */
    connection: C

    location: H.Location

    onShowMore: () => void
}

class ConnectionNodes<C extends Connection<N>, N, NP = {}> extends React.PureComponent<ConnectionNodesProps<C, N, NP>> {
    public render(): JSX.Element | null {
        const NodeComponent = this.props.nodeComponent
        const ListComponent = this.props.listComponent || 'ul'
        const HeadComponent = this.props.headComponent
        const FootComponent = this.props.footComponent
        const TotalCountSummaryComponent = this.props.totalCountSummaryComponent

        const hasNextPage = this.props.connection
            ? this.props.connection.pageInfo
                ? this.props.connection.pageInfo.hasNextPage
                : typeof this.props.connection.totalCount === 'number' &&
                  this.props.connection.nodes.length < this.props.connection.totalCount
            : false

        let totalCount: number | null = null
        if (this.props.connection) {
            if (typeof this.props.connection.totalCount === 'number') {
                totalCount = this.props.connection.totalCount
            } else if (
                // TODO(sqs): this line below is wrong because this.props.first might've just been changed and
                // this.props.connection.nodes is still the data fetched from before this.props.first was changed.
                // this causes the UI to incorrectly show "N items total" even when the count is indeterminate right
                // after the user clicks "Show more" but before the new data is loaded.
                this.props.connection.nodes.length < this.props.first ||
                (this.props.connection.nodes.length === this.props.first &&
                    this.props.connection.pageInfo &&
                    typeof this.props.connection.pageInfo.hasNextPage === 'boolean' &&
                    !this.props.connection.pageInfo.hasNextPage)
            ) {
                totalCount = this.props.connection.nodes.length
            }
        }

        let summary: React.ReactFragment | undefined
        if (
            this.props.connection &&
            (!this.props.noSummaryIfAllNodesVisible || this.props.connection.nodes.length === 0 || hasNextPage)
        ) {
            if (totalCount !== null && totalCount > 0) {
                summary = TotalCountSummaryComponent ? (
                    <TotalCountSummaryComponent totalCount={totalCount} />
                ) : (
                    <p className="filtered-connection__summary">
                        <small>
                            <span>
                                {totalCount} {pluralize(this.props.noun, totalCount, this.props.pluralNoun)}{' '}
                                {this.props.connectionQuery ? (
                                    <span>
                                        {' '}
                                        matching <strong>{this.props.connectionQuery}</strong>
                                    </span>
                                ) : (
                                    'total'
                                )}
                            </span>{' '}
                            {this.props.connection.nodes.length < totalCount &&
                                `(showing first ${this.props.connection.nodes.length})`}
                        </small>
                    </p>
                )
            } else if (this.props.connection.pageInfo?.hasNextPage) {
                // No total count to show, but it will show a 'Show more' button.
            } else if (totalCount === 0) {
                summary = this.props.emptyElement || (
                    <p className="filtered-connection__summary">
                        <small>
                            No {this.props.pluralNoun}{' '}
                            {this.props.connectionQuery && (
                                <span>
                                    matching <strong>{this.props.connectionQuery}</strong>
                                </span>
                            )}
                        </small>
                    </p>
                )
            }
        }

        const nodes = this.props.connection.nodes.map((node, index) => (
            <NodeComponent key={hasID(node) ? node.id : index} node={node} {...this.props.nodeComponentProps!} />
        ))

        return (
            <>
                {this.props.connectionQuery && summary}
                {this.props.connection && this.props.connection.nodes.length > 0 && (
                    <ListComponent className={`filtered-connection__nodes ${this.props.listClassName || ''}`}>
                        {HeadComponent && (
                            <HeadComponent
                                nodes={this.props.connection.nodes}
                                totalCount={this.props.connection.totalCount}
                            />
                        )}
                        {ListComponent === 'table' ? <tbody>{nodes}</tbody> : nodes}
                        {FootComponent && <FootComponent nodes={this.props.connection.nodes} />}
                    </ListComponent>
                )}
                {!this.props.connectionQuery && summary}
                {!this.props.loading && !this.props.noShowMore && this.props.connection && hasNextPage && (
                    <button
                        type="button"
                        className={`btn btn-secondary btn-sm filtered-connection__show-more ${
                            this.props.showMoreClassName || ''
                        }`}
                        onClick={this.onClickShowMore}
                    >
                        Show more
                    </button>
                )}
            </>
        )
    }

    private onClickShowMore = (): void => this.props.onShowMore()
}

/**
 * Fields that belong in FilteredConnectionProps and that don't depend on the type parameters. These are the fields
 * that are most likely to be needed by callers, and it's simpler for them if they are in a parameter-less type.
 */
interface FilteredConnectionDisplayProps extends ConnectionDisplayProps {
    history: H.History
    location: H.Location

    /** CSS class name for the root element. */
    className?: string

    /** Whether to display it more compactly. */
    compact?: boolean

    /**
     * An observable that upon emission causes the connection to refresh the data (by calling queryConnection).
     *
     * In most cases, it's simpler to use updateOnChange.
     */
    updates?: Observable<void>

    /**
     * Refresh the data when this value changes. It is typically constructed as a key from the query args.
     */
    updateOnChange?: string

    /** The number of items to fetch, by default. */
    defaultFirst?: number

    /** Hides the filter input field. */
    hideSearch?: boolean

    /** Autofocuses the filter input field. */
    autoFocus?: boolean

    /** Whether we will use the URL query string to reflect the filter and pagination state or not. */
    useURLQuery?: boolean

    /**
     * Filters to display next to the filter input field.
     *
     * Filters are mutually exclusive.
     */
    filters?: FilteredConnectionFilter[]

    /**
     * The filter to select by default. If not supplied, this defaults to the first
     * filter defined in the list.
     */
    defaultFilter?: string

    /** Called when a filter is selected and on initial render. */
    onFilterSelect?: (filterID: string | undefined) => void
}

/**
 * Props for the FilteredConnection component.
 *
 * @template C The GraphQL connection type, such as GQL.IRepositoryConnection.
 * @template N The node type of the GraphQL connection, such as GQL.IRepository (if C is GQL.IRepositoryConnection)
 * @template NP Props passed to `nodeComponent` in addition to `{ node: N }`
 */
interface FilteredConnectionProps<C extends Connection<N>, N, NP = {}>
    extends ConnectionPropsCommon<N, NP>,
        FilteredConnectionDisplayProps {
    /** Called to fetch the connection data to populate this component. */
    queryConnection: (args: FilteredConnectionQueryArgs) => Observable<C>

    /** Called when the queryConnection Observable emits. */
    onUpdate?: (value: C | ErrorLike | undefined) => void
}

/**
 * The arguments for the Props.queryConnection function.
 */
export interface FilteredConnectionQueryArgs {
    first?: number
    after?: string
    query?: string
}

/**
 * A filter to display next to the filter input field.
 */
export interface FilteredConnectionFilter {
    /** The UI label for the filter. */
    label: string

    /**
     * The URL string for this filter (conventionally the label, lowercased and without spaces and punctuation).
     */
    id: string

    /** An optional tooltip to display for this filter. */
    tooltip?: string

    /** Additional query args to pass to the queryConnection function when this filter is enabled. */
    args: { [name: string]: string | number | boolean }
}

interface FilteredConnectionState<C extends Connection<N>, N> extends ConnectionStateCommon {
    /** The active filter's ID (FilteredConnectionFilter.id), if any. */
    activeFilter: FilteredConnectionFilter | undefined

    /** The fetched connection data or an error (if an error occurred). */
    connectionOrError?: C | ErrorLike
}

/**
 * See https://facebook.github.io/relay/graphql/connections.htm.
 */
export interface Connection<N> {
    /**
     * The list of items (nodes) in this connection's current page.
     */
    nodes: N[]

    /**
     * The total count of items in the connection (not subject to pagination). The type accounts
     * for all known GraphQL XyzConnection types.
     *
     * If the value is a number, then the precise total count is known. If null, then the total
     * count was not precisely computable for this particular query (but might be for other queries).
     * If undefined, then the resolver never supports producing a total count.
     *
     * In the future, the UI might show `null` differently from `undefined`, but for now, the
     * distinction is maintained solely for typechecking to pass.
     */
    totalCount?: number | null

    /**
     * If set, indicates whether there is a next page. Not all GraphQL XyzConnection types return
     * pageInfo (if not, then they generally all do return totalCount). If there is a cursor to use
     * on a subsequent request it is also provided here.
     */
    pageInfo?: { hasNextPage: boolean; endCursor?: string | null }

    /**
     * If set, this error is displayed. Even when there is an error, the results are still displayed.
     */
    error?: string | null
}

/** The URL query parameter where the search query for FilteredConnection is stored. */
const QUERY_KEY = 'query'

/**
 * Displays a collection of items with filtering and pagination. It is called
 * "connection" because it is intended for use with GraphQL, which calls it that
 * (see http://graphql.org/learn/pagination/).
 *
 * @template N The node type of the GraphQL connection, such as `GQL.IRepository` (if `C` is `GQL.IRepositoryConnection`)
 * @template NP Props passed to `nodeComponent` in addition to `{ node: N }`
 * @template C The GraphQL connection type, such as `GQL.IRepositoryConnection`.
 */
export class FilteredConnection<N, NP = {}, C extends Connection<N> = Connection<N>> extends React.PureComponent<
    FilteredConnectionProps<C, N, NP>,
    FilteredConnectionState<C, N>
> {
    public static defaultProps: Partial<FilteredConnectionProps<any, any>> = {
        defaultFirst: 20,
        useURLQuery: true,
    }

    private queryInputChanges = new Subject<string>()
    private activeFilterChanges = new Subject<FilteredConnectionFilter>()
    private showMoreClicks = new Subject<void>()
    private componentUpdates = new Subject<FilteredConnectionProps<C, N, NP>>()
    private subscriptions = new Subscription()

    private filterRef: HTMLInputElement | null = null

    constructor(props: FilteredConnectionProps<C, N, NP>) {
        super(props)

        const searchParameters = new URLSearchParams(this.props.location.search)

        // Note: in the initial state, do not set `after` from the URL, as this doesn't
        // track the number of results on the previous page. This makes the count look
        // broken when coming to a page in the middle of a set of results.
        //
        // For example:
        //   (1) come to page with first = 20
        //   (2) set first and after cursor in URL
        //   (3) reload page; will skip 20 results but will display (first 20 of X)
        //
        // Instead, we use `ConnectionStateCommon.visible` to load the correct number of
        // visible results on the initial request.

        this.state = {
            loading: true,
            query: (!this.props.hideSearch && this.props.useURLQuery && searchParameters.get(QUERY_KEY)) || '',
            activeFilter:
                (this.props.useURLQuery &&
                    getFilterFromURL(searchParameters, this.props.filters, this.props.defaultFilter)) ||
                undefined,
            first: (this.props.useURLQuery && parseQueryInt(searchParameters, 'first')) || this.props.defaultFirst!,
            visible: (this.props.useURLQuery && parseQueryInt(searchParameters, 'visible')) || 0,
        }
    }

    public componentDidMount(): void {
        const activeFilterChanges = this.activeFilterChanges.pipe(
            startWith(this.state.activeFilter),
            distinctUntilChanged()
        )
        const queryChanges = this.queryInputChanges.pipe(
            distinctUntilChanged(),
            tap(query => !this.props.hideSearch && this.setState({ query })),
            debounceTime(200),
            startWith(this.state.query)
        )

        /**
         * Emits `{ forceRefresh: false }` when loading a subsequent page (keeping the existing result set),
         * and emits `{ forceRefresh: true }` on all other refresh conditions (clearing the existing result set).
         */
        const refreshRequests = new Subject<{ forceRefresh: boolean }>()

        this.subscriptions.add(
            activeFilterChanges
                .pipe(
                    tap(filter => {
                        if (this.props.onFilterSelect) {
                            this.props.onFilterSelect(filter ? filter.id : undefined)
                        }
                    })
                )
                .subscribe()
        )

        this.subscriptions.add(
            // Use this.activeFilterChanges not activeFilterChanges so that it doesn't trigger on the initial mount
            // (it doesn't need to).
            this.activeFilterChanges.subscribe(filter => this.setState({ activeFilter: filter }))
        )

        this.subscriptions.add(
            combineLatest([
                queryChanges,
                activeFilterChanges,
                refreshRequests.pipe(
                    startWith<{ forceRefresh: boolean }>({ forceRefresh: false })
                ),
            ])
                .pipe(
                    // Track whether the query or the active filter changed
                    scan<
                        [string, FilteredConnectionFilter | undefined, { forceRefresh: boolean }],
                        {
                            query: string
                            filter: FilteredConnectionFilter | undefined
                            shouldRefresh: boolean
                            queryCount: number
                        }
                    >(
                        ({ query, filter, queryCount }, [currentQuery, currentFilter, { forceRefresh }]) => ({
                            query: currentQuery,
                            filter: currentFilter,
                            shouldRefresh: forceRefresh || query !== currentQuery || filter !== currentFilter,
                            queryCount: queryCount + 1,
                        }),
                        {
                            query: this.state.query,
                            filter: undefined,
                            shouldRefresh: false,
                            queryCount: 0,
                        }
                    ),
                    switchMap(({ query, filter, shouldRefresh, queryCount }) => {
                        const result = this.props
                            .queryConnection({
                                // If this is our first query and we were supplied a value for `visible`,
                                // load that many results. If we weren't given such a value or this is a
                                // subsequent request, only ask for one page of results.
                                first: (queryCount === 1 && this.state.visible) || this.state.first,
                                after: shouldRefresh ? undefined : this.state.after,
                                query,
                                ...(filter ? filter.args : {}),
                            })
                            .pipe(
                                catchError(error => [asError(error)]),
                                map(
                                    (connectionOrError): PartialStateUpdate => ({
                                        connectionOrError,
                                        connectionQuery: query,
                                        loading: false,
                                    })
                                ),
                                share()
                            )

                        return (shouldRefresh
                            ? merge(
                                  result,
                                  of({
                                      connectionOrError: undefined,
                                      loading: true,
                                  }).pipe(delay(250), takeUntil(result))
                              )
                            : result
                        ).pipe(map(stateUpdate => ({ shouldRefresh, ...stateUpdate })))
                    }),
                    scan<PartialStateUpdate & { shouldRefresh: boolean }, PartialStateUpdate & { previousPage: N[] }>(
                        ({ previousPage }, { shouldRefresh, connectionOrError, ...rest }) => {
                            let nodes: N[] = previousPage
                            let after: string | undefined

                            if (this.props.cursorPaging && connectionOrError && !isErrorLike(connectionOrError)) {
                                if (!shouldRefresh) {
                                    connectionOrError.nodes = previousPage.concat(connectionOrError.nodes)
                                }

                                const pageInfo = connectionOrError.pageInfo
                                nodes = connectionOrError.nodes
                                after = pageInfo?.endCursor || undefined
                            }

                            return {
                                connectionOrError,
                                previousPage: nodes,
                                after,
                                ...rest,
                            }
                        },
                        {
                            previousPage: [],
                            after: undefined,
                            connectionOrError: undefined,
                            connectionQuery: undefined,
                            loading: true,
                        }
                    )
                )
                .subscribe(
                    ({ connectionOrError, previousPage, ...rest }) => {
                        if (this.props.useURLQuery) {
                            const searchFragment = this.urlQuery({ visible: previousPage.length })
                            if (this.props.location.search !== searchFragment) {
                                this.props.history.replace({
                                    search: searchFragment,
                                    hash: this.props.location.hash,
                                })
                            }
                        }
                        if (this.props.onUpdate) {
                            this.props.onUpdate(connectionOrError)
                        }
                        this.setState({ connectionOrError, ...rest })
                    },
                    error => console.error(error)
                )
        )

        type PartialStateUpdate = Pick<
            FilteredConnectionState<C, N>,
            'connectionOrError' | 'connectionQuery' | 'loading' | 'after'
        >
        this.subscriptions.add(
            this.showMoreClicks
                .pipe(
                    map(() =>
                        // If we're doing cursor paging, we rely on the `endCursor` from the previous
                        // response's `PageInfo` object to make our next request. Otherwise, we'll
                        // fallback to our legacy 'request-more' paging technique and not supply a
                        // cursor to the subsequent request.
                        ({ first: this.props.cursorPaging ? this.state.first : this.state.first * 2 })
                    )
                )
                .subscribe(({ first }) =>
                    this.setState({ first, loading: true }, () => refreshRequests.next({ forceRefresh: false }))
                )
        )

        if (this.props.updates) {
            this.subscriptions.add(
                this.props.updates.subscribe(() => {
                    this.setState({ loading: true }, () => refreshRequests.next({ forceRefresh: true }))
                })
            )
        }

        this.subscriptions.add(
            this.componentUpdates
                .pipe(
                    distinctUntilChanged((a, b) => a.updateOnChange === b.updateOnChange),
                    filter(({ updateOnChange }) => updateOnChange !== undefined),
                    // Skip the very first emission as the FilteredConnection already fetches on component creation.
                    // Otherwise, 2 requests would be triggered immediately.
                    skip(1)
                )
                .subscribe(() => {
                    this.setState({ loading: true, connectionOrError: undefined }, () =>
                        refreshRequests.next({ forceRefresh: true })
                    )
                })
        )

        // Reload collection when the query callback changes.
        this.subscriptions.add(
            this.componentUpdates
                .pipe(
                    map(({ queryConnection }) => queryConnection),
                    distinctUntilChanged(),
                    skip(1), // prevent from triggering on initial mount
                    tap(() => this.focusFilter())
                )
                .subscribe(() =>
                    this.setState({ loading: true, connectionOrError: undefined }, () =>
                        refreshRequests.next({ forceRefresh: true })
                    )
                )
        )
        this.componentUpdates.next(this.props)
    }

    private urlQuery({
        first,
        query,
        filter,
        visible,
    }: {
        first?: number
        query?: string
        filter?: FilteredConnectionFilter
        visible?: number
    }): string {
        if (!first) {
            first = this.state.first
        }
        if (!query) {
            query = this.state.query
        }
        if (!filter) {
            filter = this.state.activeFilter
        }
        const searchParameters = new URLSearchParams(this.props.location.search)
        if (query) {
            searchParameters.set(QUERY_KEY, query)
        }

        if (first !== this.props.defaultFirst) {
            searchParameters.set('first', String(first))
        }
        if (filter && this.props.filters) {
            if (filter !== this.props.filters[0]) {
                searchParameters.set('filter', filter.id)
            } else {
                searchParameters.delete('filter')
            }
        }
        if (visible !== 0 && visible !== first) {
            searchParameters.set('visible', String(visible))
        }
        return searchParameters.toString()
    }

    public componentDidUpdate(): void {
        this.componentUpdates.next(this.props)
    }

    public componentWillUnmount(): void {
        this.subscriptions.unsubscribe()
    }

    public render(): JSX.Element | null {
        const errors: string[] = []
        if (isErrorLike(this.state.connectionOrError)) {
            errors.push(...uniq(this.state.connectionOrError.message.split('\n')))
        }
        if (
            this.state.connectionOrError &&
            !isErrorLike(this.state.connectionOrError) &&
            this.state.connectionOrError.error
        ) {
            errors.push(this.state.connectionOrError.error)
        }

        const compactnessClass = `filtered-connection--${this.props.compact ? 'compact' : 'noncompact'}`
        return (
            <div
                className={`filtered-connection test-filtered-connection ${compactnessClass} ${
                    this.props.className || ''
                }`}
            >
                {(!this.props.hideSearch || this.props.filters) && (
                    <Form className="filtered-connection__form" onSubmit={this.onSubmit}>
                        {!this.props.hideSearch && (
                            <input
                                className="form-control filtered-connection__filter"
                                type="search"
                                placeholder={`Search ${this.props.pluralNoun}...`}
                                name="query"
                                value={this.state.query}
                                onChange={this.onChange}
                                autoFocus={this.props.autoFocus}
                                autoComplete="off"
                                autoCorrect="off"
                                autoCapitalize="off"
                                ref={this.setFilterRef}
                                spellCheck={false}
                            />
                        )}
                        {this.props.filters && this.state.activeFilter ? (
                            <FilteredConnectionFilterControl
                                filters={this.props.filters}
                                onDidSelectFilter={this.onDidSelectFilter}
                                value={this.state.activeFilter.id}
                            >
                                {this.props.additionalFilterElement}
                            </FilteredConnectionFilterControl>
                        ) : (
                            this.props.additionalFilterElement
                        )}
                    </Form>
                )}
                {errors.length > 0 && (
                    <div className="alert alert-danger filtered-connection__error">
                        {errors.map((error, index) => (
                            <React.Fragment key={index}>
                                <ErrorMessage error={error} history={this.props.history} />
                            </React.Fragment>
                        ))}
                    </div>
                )}
                {this.state.connectionOrError && !isErrorLike(this.state.connectionOrError) && (
                    <ConnectionNodes
                        connection={this.state.connectionOrError}
                        loading={this.state.loading}
                        connectionQuery={this.state.connectionQuery}
                        first={this.state.first}
                        query={this.state.query}
                        noun={this.props.noun}
                        pluralNoun={this.props.pluralNoun}
                        listComponent={this.props.listComponent}
                        listClassName={this.props.listClassName}
                        headComponent={this.props.headComponent}
                        footComponent={this.props.footComponent}
                        showMoreClassName={this.props.showMoreClassName}
                        nodeComponent={this.props.nodeComponent}
                        nodeComponentProps={this.props.nodeComponentProps}
                        noShowMore={this.props.noShowMore}
                        noSummaryIfAllNodesVisible={this.props.noSummaryIfAllNodesVisible}
                        onShowMore={this.onClickShowMore}
                        location={this.props.location}
                        emptyElement={this.props.emptyElement}
                        totalCountSummaryComponent={this.props.totalCountSummaryComponent}
                    />
                )}
                {this.state.loading && (
                    <span className="filtered-connection__loader test-filtered-connection__loader">
                        <LoadingSpinner className="icon-inline" />
                    </span>
                )}
            </div>
        )
    }

    private setFilterRef = (element: HTMLInputElement | null): void => {
        this.filterRef = element
        if (element && this.props.autoFocus) {
            // TODO(sqs): The 30 msec delay is needed, or else the input is not
            // reliably focused. Find out why.
            setTimeout(() => element.focus(), 30)
        }
    }

    private focusFilter = (): void => {
        if (this.filterRef) {
            this.filterRef.focus()
        }
    }

    private onSubmit: React.FormEventHandler<HTMLFormElement> = event => {
        // Do nothing. The <input onChange> handler will pick up any changes shortly.
        event.preventDefault()
    }

    private onChange: React.ChangeEventHandler<HTMLInputElement> = event => {
        this.queryInputChanges.next(event.currentTarget.value)
    }

    private onDidSelectFilter = (filter: FilteredConnectionFilter): void => this.activeFilterChanges.next(filter)

    private onClickShowMore = (): void => {
        this.showMoreClicks.next()
    }
}

function parseQueryInt(searchParameters: URLSearchParams, name: string): number | null {
    const valueString = searchParameters.get(name)
    if (valueString === null) {
        return null
    }
    const valueNumber = parseInt(valueString, 10)
    if (valueNumber > 0) {
        return valueNumber
    }
    return null
}

function getFilterFromURL(
    searchParameters: URLSearchParams,
    filters: FilteredConnectionFilter[] | undefined,
    defaultFilterId: string | undefined
): FilteredConnectionFilter | undefined {
    if (filters === undefined || filters.length === 0) {
        return undefined
    }
    const id = searchParameters.get('filter') || defaultFilterId
    if (id !== null) {
        const filter = filters.find(filter => filter.id === id)
        if (filter) {
            return filter
        }
    }
    return filters[0] // default
}
