import { DecoratorFn, Meta, Story } from '@storybook/react'
import { createBrowserHistory } from 'history'
import { EMPTY, NEVER, of } from 'rxjs'
import sinon from 'sinon'

import { SearchQueryStateStoreProvider } from '@sourcegraph/search'
import { AggregateStreamingSearchResults } from '@sourcegraph/shared/src/search/stream'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    extensionsController,
    HIGHLIGHTED_FILE_LINES_LONG,
    MULTIPLE_SEARCH_RESULT,
    REPO_MATCH_RESULTS_WITH_METADATA,
} from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { AuthenticatedUser } from '../../auth'
import { WebStory } from '../../components/WebStory'
import { useExperimentalFeatures, useNavbarQueryState } from '../../stores'

import { StreamingSearchResults, StreamingSearchResultsProps } from './StreamingSearchResults'

const history = createBrowserHistory()
history.replace({ search: 'q=r:golang/oauth2+test+f:travis' })

const streamingSearchResult: AggregateStreamingSearchResults = {
    state: 'complete',
    results: [...MULTIPLE_SEARCH_RESULT.results, ...REPO_MATCH_RESULTS_WITH_METADATA],
    filters: MULTIPLE_SEARCH_RESULT.filters,
    progress: {
        durationMs: 500,
        matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
        skipped: [],
    },
}

const defaultProps: StreamingSearchResultsProps = {
    extensionsController,
    telemetryService: NOOP_TELEMETRY_SERVICE,

    history,
    location: history.location,
    authenticatedUser: {
        url: '/users/alice',
        displayName: 'Alice',
        username: 'alice',
        email: 'alice@email.test',
    } as AuthenticatedUser,
    isLightTheme: true,

    settingsCascade: {
        subjects: null,
        final: null,
    },
    platformContext: { forceUpdateTooltip: sinon.spy(), settings: NEVER, requestGraphQL: () => EMPTY },

    streamSearch: () => of(streamingSearchResult),

    fetchHighlightedFileLineRanges: () => of(HIGHLIGHTED_FILE_LINES_LONG),
    isSourcegraphDotCom: false,
    searchContextsEnabled: true,
}

const decorator: DecoratorFn = Story => {
    useExperimentalFeatures.setState({ codeMonitoring: true, showSearchContext: true })
    useNavbarQueryState.setState({ searchQueryFromURL: 'r:golang/oauth2 test f:travis' })
    return <Story />
}

const config: Meta = {
    title: 'web/search/results/StreamingSearchResults',
    decorators: [decorator],
    parameters: {
        chromatic: { viewports: [577, 769, 993], disableSnapshot: false },
    },
}

export default config

export const StandardRender: Story = () => (
    <WebStory>
        {() => (
            <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                <StreamingSearchResults {...defaultProps} />
            </SearchQueryStateStoreProvider>
        )}
    </WebStory>
)

StandardRender.storyName = 'standard render'

export const UnauthenticatedUserStandardRender: Story = () => (
    <WebStory>
        {() => (
            <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                <StreamingSearchResults {...defaultProps} authenticatedUser={null} />
            </SearchQueryStateStoreProvider>
        )}
    </WebStory>
)

UnauthenticatedUserStandardRender.storyName = 'unauthenticated user standard render'

export const NoResults: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'complete',
        results: [],
        filters: [],
        progress: {
            durationMs: 500,
            matchCount: 0,
            skipped: [],
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

NoResults.storyName = 'no results'

export const SearchWithQuotes: Story = () => {
    useNavbarQueryState.setState({ searchQueryFromURL: 'r:golang/oauth2 test f:travis "test"' })

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

SearchWithQuotes.storyName = 'search with quotes'

export const DidYouMean: Story = () => {
    useNavbarQueryState.setState({ searchQueryFromURL: 'javascript test' })

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

DidYouMean.storyName = 'did you mean'

export const ProgressWithWarning: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'complete',
        results: MULTIPLE_SEARCH_RESULT.results,
        filters: MULTIPLE_SEARCH_RESULT.filters,
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [
                {
                    reason: 'excluded-fork',
                    message: '10k forked repositories excluded',
                    severity: 'info',
                    title: '10k forked repositories excluded',
                    suggested: {
                        title: 'forked:yes',
                        queryExpression: 'forked:yes',
                    },
                },
                {
                    reason: 'excluded-archive',
                    message: '60k archived repositories excluded',
                    severity: 'info',
                    title: '60k archived repositories excluded',
                    suggested: {
                        title: 'archived:yes',
                        queryExpression: 'archived:yes',
                    },
                },
                {
                    reason: 'shard-timedout',
                    message: 'Search timed out',
                    severity: 'warn',
                    title: 'Search timed out',
                    suggested: {
                        title: 'timeout:2m',
                        queryExpression: 'timeout:2m',
                    },
                },
            ],
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

ProgressWithWarning.storyName = 'progress with warnings'

export const LoadingWithNoResults: Story = () => (
    <WebStory>
        {() => (
            <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                <StreamingSearchResults {...defaultProps} streamSearch={() => NEVER} />
            </SearchQueryStateStoreProvider>
        )}
    </WebStory>
)

LoadingWithNoResults.storyName = 'loading with no results'

export const LoadingWithSomeResults: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'loading',
        results: MULTIPLE_SEARCH_RESULT.results,
        filters: MULTIPLE_SEARCH_RESULT.filters,
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [],
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

LoadingWithSomeResults.storyName = 'loading with some results'

export const ServerSideAlert: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'complete',
        results: MULTIPLE_SEARCH_RESULT.results,
        filters: MULTIPLE_SEARCH_RESULT.filters,
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [],
        },
        alert: {
            proposedQueries: [{ query: 'test', description: 'new query' }],
            title: 'Test alert',
            description: 'This is an alert',
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

ServerSideAlert.storyName = 'server-side alert'

export const ServerSideAlertNoResults: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'complete',
        results: [],
        filters: [],
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [],
        },
        alert: {
            proposedQueries: [{ query: 'test', description: 'Test query' }],
            title: 'Test Alert',
            description: 'This is a test alert',
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

ServerSideAlertNoResults.storyName = 'server-side alert with no results'

export const ErrorWithNoResults: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'error',
        results: [],
        filters: [],
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [],
        },
        error: new Error('test error'),
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

ErrorWithNoResults.storyName = 'error with no results'

export const ErrorWithSomeResults: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'error',
        results: MULTIPLE_SEARCH_RESULT.results,
        filters: MULTIPLE_SEARCH_RESULT.filters,
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [],
        },
        error: new Error('test error'),
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

ErrorWithSomeResults.storyName = 'error with some results'

export const LimitHitWithSomeResults: Story = () => {
    const result: AggregateStreamingSearchResults = {
        state: 'complete',
        results: MULTIPLE_SEARCH_RESULT.results,
        filters: MULTIPLE_SEARCH_RESULT.filters,
        progress: {
            durationMs: 500,
            matchCount: MULTIPLE_SEARCH_RESULT.progress.matchCount,
            skipped: [
                {
                    reason: 'document-match-limit',
                    message: 'result limit hit',
                    severity: 'info',
                    title: 'result limit hit',
                },
            ],
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                    <StreamingSearchResults {...defaultProps} streamSearch={() => of(result)} />
                </SearchQueryStateStoreProvider>
            )}
        </WebStory>
    )
}

LimitHitWithSomeResults.storyName = 'limit hit with some results'
