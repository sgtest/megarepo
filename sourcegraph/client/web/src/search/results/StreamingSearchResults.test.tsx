import React from 'react'

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createBrowserHistory } from 'history'
import { BrowserRouter } from 'react-router-dom'
import { CompatRouter } from 'react-router-dom-v5-compat'
import { EMPTY, NEVER, of } from 'rxjs'
import sinon from 'sinon'

import { SearchQueryStateStoreProvider } from '@sourcegraph/search'
import { GitRefType, SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'
import { AggregateStreamingSearchResults, Skipped } from '@sourcegraph/shared/src/search/stream'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'
import {
    COLLAPSABLE_SEARCH_RESULT,
    extensionsController,
    HIGHLIGHTED_FILE_LINES_REQUEST,
    MULTIPLE_SEARCH_RESULT,
    REPO_MATCH_RESULT,
    RESULT,
} from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { AuthenticatedUser } from '../../auth'
import { useExperimentalFeatures, useNavbarQueryState } from '../../stores'
import * as helpers from '../helpers'

import { generateMockedResponses } from './sidebar/Revisions.mocks'
import { StreamingSearchResults, StreamingSearchResultsProps } from './StreamingSearchResults'

describe('StreamingSearchResults', () => {
    const history = createBrowserHistory()

    const streamingSearchResult = MULTIPLE_SEARCH_RESULT

    const defaultProps: StreamingSearchResultsProps = {
        extensionsController,
        telemetryService: NOOP_TELEMETRY_SERVICE,

        history,
        location: history.location,
        authenticatedUser: null,

        settingsCascade: {
            subjects: null,
            final: null,
        },
        platformContext: { settings: NEVER, requestGraphQL: () => EMPTY, sourcegraphURL: 'https://sourcegraph.com' },

        streamSearch: () => of(MULTIPLE_SEARCH_RESULT),

        fetchHighlightedFileLineRanges: HIGHLIGHTED_FILE_LINES_REQUEST,
        isLightTheme: true,
        isSourcegraphDotCom: false,
        searchContextsEnabled: true,
    }

    const revisionsMockResponses = generateMockedResponses(GitRefType.GIT_BRANCH, 5, 'github.com/golang/oauth2')

    function renderWrapper(component: React.ReactElement<StreamingSearchResultsProps>) {
        return render(
            <BrowserRouter>
                <CompatRouter>
                    <MockedTestProvider mocks={revisionsMockResponses}>
                        <SearchQueryStateStoreProvider useSearchQueryState={useNavbarQueryState}>
                            {component}
                        </SearchQueryStateStoreProvider>
                    </MockedTestProvider>
                </CompatRouter>
            </BrowserRouter>
        )
    }

    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
    const mockUser = {
        id: 'userID',
        username: 'username',
        email: 'user@me.com',
        siteAdmin: true,
    } as AuthenticatedUser

    // Modified from https://sourcegraph.com/github.com/reach/reach-ui@26c826684729e51e45eef29aa4316df19c0e2c03/-/blob/test/utils.tsx?L105
    // userEvent.click does not work for Reach menu items. Use this function from Reach's official test code instead.
    function simualateMenuItemClick(element: HTMLElement) {
        fireEvent.mouseEnter(element)
        fireEvent.keyDown(element, { key: ' ' })
        fireEvent.keyUp(element, { key: ' ' })
    }

    beforeEach(() => {
        useNavbarQueryState.setState({
            searchCaseSensitivity: false,
            searchQueryFromURL: 'r:golang/oauth2 test f:travis',
        })
        useExperimentalFeatures.setState({ showSearchContext: true, codeMonitoring: false })
        window.context = {
            enableLegacyExtensions: false,
        } as any
    })

    it('should call streaming search API with the right parameters from URL', async () => {
        useNavbarQueryState.setState({ searchCaseSensitivity: true, searchPatternType: SearchPatternType.regexp })
        const searchSpy = sinon.spy(defaultProps.streamSearch)

        renderWrapper(<StreamingSearchResults {...defaultProps} streamSearch={searchSpy} />)

        sinon.assert.calledOnce(searchSpy)
        const call = searchSpy.getCall(0)
        // We have to extract the query from the observable since we can't directly compare observables
        const receivedQuery = await call.args[0].toPromise()
        const receivedOptions = call.args[1]

        expect(receivedQuery).toEqual('r:golang/oauth2 test f:travis')
        expect(receivedOptions).toEqual({
            version: 'V3',
            patternType: SearchPatternType.regexp,
            caseSensitive: true,
            trace: undefined,
            chunkMatches: true,
        })
    })

    it('should render progress with data from API', () => {
        renderWrapper(<StreamingSearchResults {...defaultProps} />)

        // Dropdown not in doc for progress.skipped === []
        expect(screen.queryByTestId('streaming-progress-dropdown')).not.toBeInTheDocument()
        const expectedString = `${streamingSearchResult.progress.matchCount} results in ${(
            streamingSearchResult.progress.durationMs / 1000
        ).toFixed(2)}s`
        expect(screen.getAllByTestId('streaming-progress-count')[0]).toHaveTextContent(expectedString)
    })

    it('should expand and collapse results when event from infobar is triggered', async () => {
        renderWrapper(<StreamingSearchResults {...defaultProps} streamSearch={() => of(COLLAPSABLE_SEARCH_RESULT)} />)

        screen
            .getAllByTestId('file-search-result')
            .map(element => expect(element).toHaveAttribute('data-expanded', 'false'))

        userEvent.click(await screen.findByLabelText(/Open search actions menu/))
        simualateMenuItemClick(await screen.findByText(/Expand all/, { selector: '[role=menuitem]' }))

        screen
            .getAllByTestId('file-search-result')
            .map(element => expect(element).toHaveAttribute('data-expanded', 'true'))

        userEvent.click(await screen.findByLabelText(/Open search actions menu/))
        simualateMenuItemClick(await screen.findByText(/Collapse all/, { selector: '[role=menuitem]' }))

        screen
            .getAllByTestId('file-search-result')
            .map(element => expect(element).toHaveAttribute('data-expanded', 'false'))
    })

    it('should render correct components for file match and repository match', () => {
        const results: AggregateStreamingSearchResults = {
            ...streamingSearchResult,
            results: [RESULT, REPO_MATCH_RESULT],
        }
        renderWrapper(<StreamingSearchResults {...defaultProps} streamSearch={() => of(results)} />)
        expect(screen.getAllByTestId('result-container').length).toBe(2)
        expect(screen.getByTestId('search-repo-result')).toBeVisible()

        expect(screen.getAllByTestId('result-container')[0]).toHaveAttribute('data-result-type', 'content')
        expect(screen.getAllByTestId('result-container')[1]).toHaveAttribute('data-result-type', 'repo')
    })

    it('should log view, query, and results fetched events', () => {
        const logSpy = sinon.spy()
        const logViewEventSpy = sinon.spy()
        const telemetryService = {
            ...NOOP_TELEMETRY_SERVICE,
            log: logSpy,
            logViewEvent: logViewEventSpy,
        }

        renderWrapper(<StreamingSearchResults {...defaultProps} telemetryService={telemetryService} />)

        sinon.assert.calledOnceWithExactly(logViewEventSpy, 'SearchResults')
        sinon.assert.calledWith(logSpy, 'SearchResultsQueried')
        sinon.assert.calledWith(logSpy, 'SearchResultsFetched')
    })

    it('should log event when clicking on search result', () => {
        const logSpy = sinon.spy()
        const telemetryService = {
            ...NOOP_TELEMETRY_SERVICE,
            log: logSpy,
        }

        renderWrapper(<StreamingSearchResults {...defaultProps} telemetryService={telemetryService} />)

        userEvent.click(screen.getAllByTestId('result-container')[0])
        sinon.assert.calledWith(logSpy, 'SearchResultClicked')
    })

    it('should not show saved search modal on first load', () => {
        renderWrapper(<StreamingSearchResults {...defaultProps} />)
        expect(screen.queryByTestId('saved-search-modal')).not.toBeInTheDocument()
    })

    it('should open and close saved search modal if events trigger', async () => {
        renderWrapper(<StreamingSearchResults {...defaultProps} authenticatedUser={mockUser} />)
        userEvent.click(await screen.findByLabelText(/Open search actions menu/))
        simualateMenuItemClick(await screen.findByText(/Save search/, { selector: '[role=menuitem]' }))

        fireEvent.keyDown(await screen.findByText(/Save search query to:/), {
            key: 'Escape',
            code: 'Escape',
            keyCode: 27,
            charCode: 27,
        })

        expect(screen.queryByText(/Save search query to:/)).not.toBeInTheDocument()
    })

    it('should start a new search with added params when onSearchAgain event is triggered', async () => {
        const submitSearchMock = jest.spyOn(helpers, 'submitSearch').mockImplementation(() => {})
        const tests = [
            {
                parsedSearchQuery: 'r:golang/oauth2 test f:travis',
                skipReason: ['document-match-limit', 'excluded-archive', 'shard-timedout'] as Skipped['reason'][],
                additionalProperties: ['count:1000', 'archived:yes', 'timeout:2m'],
                want: 'r:golang/oauth2 test f:travis count:1000 archived:yes timeout:2m',
            },
            {
                parsedSearchQuery: 'r:golang/oauth2 test f:travis count:50',
                skipReason: ['document-match-limit', 'excluded-archive', 'shard-timedout'] as Skipped['reason'][],
                additionalProperties: ['count:1000', 'archived:yes', 'timeout:2m'],
                want: 'r:golang/oauth2 test f:travis count:1000 archived:yes timeout:2m',
            },
            {
                parsedSearchQuery: 'r:golang/oauth2 (foo count:1) or (bar count:2)',
                skipReason: ['document-match-limit', 'excluded-fork'] as Skipped['reason'][],
                additionalProperties: ['count:1000', 'fork:yes'],
                want: 'r:golang/oauth2 (foo count:1000) or (bar count:1000) fork:yes',
            },
        ]

        ;(global as any).document.createRange = () => ({
            setStart: () => {},
            setEnd: () => {},
            commonAncestorContainer: {
                nodeName: 'BODY',
                ownerDocument: document,
            },
        })

        for (const [index, test] of tests.entries()) {
            cleanup()

            const results: AggregateStreamingSearchResults = {
                ...streamingSearchResult,
                progress: {
                    ...streamingSearchResult.progress,
                    skipped: test.additionalProperties.map((property, propertyIndex) => ({
                        reason: test.skipReason[propertyIndex],
                        message: property,
                        severity: 'info',
                        title: property,
                        suggested: {
                            title: property,
                            queryExpression: property,
                        },
                    })),
                },
            }

            useNavbarQueryState.setState({ searchQueryFromURL: test.parsedSearchQuery })

            renderWrapper(<StreamingSearchResults {...defaultProps} streamSearch={() => of(results)} />)

            userEvent.click((await screen.findAllByText(/results in/i))[0])
            const allChecks = await screen.findAllByTestId(/^streaming-progress-skipped-suggest-check/)

            for (const check of allChecks) {
                userEvent.click(check, undefined, { skipPointerEventsCheck: true })
            }

            userEvent.click(await screen.findByText(/search again/i, { selector: 'button[type=submit]' }), undefined, {
                skipPointerEventsCheck: true,
            })

            expect(helpers.submitSearch).toBeCalledTimes(index + 1)
            const args = submitSearchMock.mock.calls[index][0]
            expect(args.query).toBe(test.want)
        }
    })
})
