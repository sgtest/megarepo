import { render } from '@testing-library/react'
import { createBrowserHistory } from 'history'
import React from 'react'
import { BrowserRouter } from 'react-router-dom'
import { NEVER } from 'rxjs'
import sinon from 'sinon'

import { extensionsController } from '@sourcegraph/shared/src/util/searchTestHelpers'

import { SearchPatternType } from './graphql-operations'
import { Layout, LayoutProps } from './Layout'

jest.mock('./theme', () => ({
    useTheme: () => ({
        isLightTheme: true,
        themePreference: 'system',
        onThemePreferenceChange: () => {},
    }),
}))

describe('Layout', () => {
    const defaultProps: LayoutProps = ({
        // Parsed query components
        parsedSearchQuery: 'r:golang/oauth2 test f:travis',
        setParsedSearchQuery: () => {},
        patternType: SearchPatternType.literal,
        setPatternType: () => {},
        caseSensitive: false,
        setCaseSensitivity: () => {},

        // Other minimum props required to render
        routes: [],
        navbarSearchQueryState: { query: '' },
        onNavbarQueryChange: () => {},
        settingsCascade: {
            subjects: null,
            final: null,
        },
        keyboardShortcuts: [],
        extensionsController,
        platformContext: { forceUpdateTooltip: () => {}, settings: NEVER },
    } as unknown) as LayoutProps

    beforeEach(() => {
        const root = document.createElement('div')
        root.id = 'root'
        document.body.append(root)
    })

    afterEach(() => {
        document.querySelector('#root')?.remove()
    })

    it('should update parsedSearchQuery if different between URL and context', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=r:golang/oauth2+test+f:travis2&patternType=regexp' })

        const setParsedSearchQuery = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    setParsedSearchQuery={setParsedSearchQuery}
                />
            </BrowserRouter>
        )

        sinon.assert.called(setParsedSearchQuery)
        sinon.assert.calledWith(setParsedSearchQuery, 'r:golang/oauth2 test f:travis2')
    })

    it('should not update parsedSearchQuery if URL and context are the same', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=r:golang/oauth2+test+f:travis&patternType=regexp' })

        const setParsedSearchQuery = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    setParsedSearchQuery={setParsedSearchQuery}
                />
            </BrowserRouter>
        )

        sinon.assert.notCalled(setParsedSearchQuery)
    })

    it('should update parsedSearchQuery if changing to empty', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=&patternType=regexp' })

        const setParsedSearchQuery = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    setParsedSearchQuery={setParsedSearchQuery}
                />
            </BrowserRouter>
        )

        sinon.assert.called(setParsedSearchQuery)
        sinon.assert.calledWith(setParsedSearchQuery, '')
    })

    it('should update patternType if different between URL and context', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=r:golang/oauth2+test+f:travis&patternType=regexp' })

        const setPatternTypeSpy = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    patternType={SearchPatternType.literal}
                    setPatternType={setPatternTypeSpy}
                />
            </BrowserRouter>
        )

        sinon.assert.called(setPatternTypeSpy)
        sinon.assert.calledWith(setPatternTypeSpy, SearchPatternType.regexp)
    })

    it('should not update patternType if URL and context are the same', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=r:golang/oauth2+test+f:travis&patternType=regexp' })

        const setPatternTypeSpy = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    patternType={SearchPatternType.regexp}
                    setPatternType={setPatternTypeSpy}
                />
            </BrowserRouter>
        )

        sinon.assert.notCalled(setPatternTypeSpy)
    })

    it('should not update patternType if query is empty', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=&patternType=regexp' })

        const setPatternTypeSpy = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    patternType={SearchPatternType.literal}
                    setPatternType={setPatternTypeSpy}
                />
            </BrowserRouter>
        )

        sinon.assert.notCalled(setPatternTypeSpy)
    })

    it('should update caseSensitive if different between URL and context', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=r:golang/oauth2+test+f:travis case:yes' })

        const setCaseSensitivitySpy = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    caseSensitive={false}
                    setCaseSensitivity={setCaseSensitivitySpy}
                />
            </BrowserRouter>
        )

        sinon.assert.called(setCaseSensitivitySpy)
        sinon.assert.calledWith(setCaseSensitivitySpy, true)
    })

    it('should not update caseSensitive if URL and context are the same', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=r:golang/oauth2+test+f:travis+case:yes' })

        const setCaseSensitivitySpy = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    caseSensitive={true}
                    setCaseSensitivity={setCaseSensitivitySpy}
                />
            </BrowserRouter>
        )

        sinon.assert.notCalled(setCaseSensitivitySpy)
    })

    it('should not update caseSensitive if query is empty', () => {
        const history = createBrowserHistory()
        history.replace({ search: 'q=case:yes' })

        const setCaseSensitivitySpy = sinon.spy()

        render(
            <BrowserRouter>
                <Layout
                    {...defaultProps}
                    history={history}
                    location={history.location}
                    caseSensitive={false}
                    setCaseSensitivity={setCaseSensitivitySpy}
                />
            </BrowserRouter>
        )

        sinon.assert.notCalled(setCaseSensitivitySpy)
    })
})
