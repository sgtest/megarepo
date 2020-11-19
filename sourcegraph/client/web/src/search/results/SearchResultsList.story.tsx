import { createBrowserHistory } from 'history'
import * as React from 'react'
import _VisibilitySensor from 'react-visibility-sensor'
import sinon from 'sinon'
import { NOOP_TELEMETRY_SERVICE } from '../../../../shared/src/telemetry/telemetryService'
import {
    extensionsController,
    HIGHLIGHTED_FILE_LINES_REQUEST,
    SEARCH_REQUEST,
} from '../../../../shared/src/util/searchTestHelpers'
import { SearchResultsList, SearchResultsListProps } from './SearchResultsList'
import { NEVER, of } from 'rxjs'
import { SearchPatternType } from '../../../../shared/src/graphql-operations'
import { storiesOf } from '@storybook/react'
import { WebStory } from '../../components/WebStory'
import { ISearchResults } from '../../../../shared/src/graphql/schema'

const history = createBrowserHistory()
history.replace({ search: 'q=r:golang/oauth2+test+f:travis' })

const defaultProps: SearchResultsListProps = {
    location: history.location,
    history,
    authenticatedUser: null,
    isSourcegraphDotCom: false,
    deployType: 'dev',

    resultsOrError: SEARCH_REQUEST(),
    onShowMoreResultsClick: sinon.spy(),

    allExpanded: true,
    onExpandAllResultsToggle: sinon.spy(),

    showSavedQueryModal: false,
    onSavedQueryModalClose: sinon.spy(),
    onDidCreateSavedQuery: sinon.spy(),
    onSaveQueryClick: sinon.spy(),
    didSave: false,

    fetchHighlightedFileLines: HIGHLIGHTED_FILE_LINES_REQUEST,

    isLightTheme: true,
    settingsCascade: {
        subjects: null,
        final: null,
    },
    extensionsController: { executeCommand: sinon.spy(), services: extensionsController.services },
    platformContext: { forceUpdateTooltip: sinon.spy(), settings: NEVER },
    telemetryService: NOOP_TELEMETRY_SERVICE,
    patternType: SearchPatternType.regexp,
    setPatternType: sinon.spy(),
    caseSensitive: false,
    setCaseSensitivity: sinon.spy(),

    interactiveSearchMode: false,
    filtersInQuery: {},
    toggleSearchMode: sinon.fake(),
    onFiltersInQueryChange: sinon.fake(),
    splitSearchModes: false,
    versionContext: undefined,

    navbarSearchQueryState: { query: '', cursorPosition: 0 },
    searchStreaming: false,

    shouldDisplayPerformanceWarning: () => of(false),
}

const { add } = storiesOf('web/search/results/SearchResultsList', module).addParameters({
    chromatic: { viewports: [769, 993] },
})

add('loading', () => <WebStory>{() => <SearchResultsList {...defaultProps} resultsOrError={undefined} />}</WebStory>)

add('single result', () => <WebStory>{() => <SearchResultsList {...defaultProps} />}</WebStory>)

add('no results with quote tip in infobar', () => {
    const resultsOrError: ISearchResults = {
        ...(defaultProps.resultsOrError as ISearchResults),
        results: [],
        matchCount: 0,
        approximateResultCount: '0',
    }

    const location = {
        ...history.location,
        search: 'q="test"',
    }

    return (
        <WebStory>
            {() => (
                <SearchResultsList
                    {...defaultProps}
                    resultsOrError={resultsOrError}
                    patternType={SearchPatternType.literal}
                    location={location}
                />
            )}
        </WebStory>
    )
})

add('error', () => (
    <WebStory>
        {() => <SearchResultsList {...defaultProps} resultsOrError={{ message: 'test error', name: 'TestError' }} />}
    </WebStory>
))

add('show performance warning', () => {
    const shouldDisplayPerformanceWarning = () => of(true)

    return (
        <WebStory>
            {() => (
                <SearchResultsList
                    {...defaultProps}
                    shouldDisplayPerformanceWarning={shouldDisplayPerformanceWarning}
                />
            )}
        </WebStory>
    )
})

add('show server side alert', () => {
    const shouldDisplayPerformanceWarning = () => of(true)
    const resultsOrError: ISearchResults = {
        ...(defaultProps.resultsOrError as ISearchResults),
        alert: {
            __typename: 'SearchAlert',
            description: 'This is a test alert',
            proposedQueries: [{ __typename: 'SearchQueryDescription', description: 'Test query', query: 'test' }],
            title: 'Test Alert',
        },
    }

    return (
        <WebStory>
            {() => (
                <SearchResultsList
                    {...defaultProps}
                    resultsOrError={resultsOrError}
                    shouldDisplayPerformanceWarning={shouldDisplayPerformanceWarning}
                />
            )}
        </WebStory>
    )
})
