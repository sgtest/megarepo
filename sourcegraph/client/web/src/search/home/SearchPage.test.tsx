import { cleanup, render } from '@testing-library/react'
import { createMemoryHistory } from 'history'
import React from 'react'
import { of } from 'rxjs'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    mockFetchAutoDefinedSearchContexts,
    mockFetchSearchContexts,
    mockGetUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/testing/searchContexts/testHelpers'
import { extensionsController } from '@sourcegraph/shared/src/util/searchTestHelpers'

import { useExperimentalFeatures } from '../../stores'
import { ThemePreference } from '../../stores/themeState'
import { authUser } from '../panels/utils'

import { SearchPage, SearchPageProps } from './SearchPage'

// Mock the Monaco input box to make this a shallow test
jest.mock('./SearchPageInput', () => ({
    SearchPageInput: () => null,
}))

// Uses import.meta.url, which is a SyntaxError when used outside of ES Modules (Jest runs tests as
// CommonJS).
jest.mock('./LoggedOutHomepage.constants', () => ({
    fonts: [],
    exampleQueries: [],
    exampleNotebooks: [],
}))

describe('SearchPage', () => {
    afterAll(cleanup)

    let container: HTMLElement

    const history = createMemoryHistory()
    const defaultProps: SearchPageProps = {
        isSourcegraphDotCom: false,
        settingsCascade: {
            final: null,
            subjects: null,
        },
        location: history.location,
        history,
        extensionsController,
        telemetryService: NOOP_TELEMETRY_SERVICE,
        themePreference: ThemePreference.Light,
        onThemePreferenceChange: () => undefined,
        authenticatedUser: authUser,
        globbing: false,
        platformContext: {} as any,
        keyboardShortcuts: [],
        searchContextsEnabled: true,
        selectedSearchContextSpec: '',
        setSelectedSearchContextSpec: () => {},
        defaultSearchContextSpec: '',
        isLightTheme: true,
        fetchSavedSearches: () => of([]),
        fetchRecentSearches: () => of({ nodes: [], totalCount: 0, pageInfo: { hasNextPage: false, endCursor: null } }),
        fetchRecentFileViews: () => of({ nodes: [], totalCount: 0, pageInfo: { hasNextPage: false, endCursor: null } }),
        fetchAutoDefinedSearchContexts: mockFetchAutoDefinedSearchContexts(),
        fetchSearchContexts: mockFetchSearchContexts,
        hasUserAddedRepositories: false,
        hasUserAddedExternalServices: false,
        getUserSearchContextNamespaces: mockGetUserSearchContextNamespaces,
        featureFlags: new Map(),
        extensionViews: () => null,
    }

    it('should not show home panels if on Sourcegraph.com and showEnterpriseHomePanels disabled', () => {
        container = render(<SearchPage {...defaultProps} isSourcegraphDotCom={true} />).container
        const homePanels = container.querySelector('[data-testid="home-panels"]')
        expect(homePanels).not.toBeInTheDocument()
    })

    it('should show home panels if on Sourcegraph.com and showEnterpriseHomePanels enabled', () => {
        useExperimentalFeatures.setState({ showEnterpriseHomePanels: true })

        container = render(<SearchPage {...defaultProps} isSourcegraphDotCom={true} />).container
        const homePanels = container.querySelector('[data-testid="home-panels"]')
        expect(homePanels).toBeVisible()
    })

    it('should show home panels if on Sourcegraph.com and showEnterpriseHomePanels enabled with user logged out', () => {
        useExperimentalFeatures.setState({ showEnterpriseHomePanels: true })

        container = render(<SearchPage {...defaultProps} isSourcegraphDotCom={true} authenticatedUser={null} />)
            .container
        const homePanels = container.querySelector('[data-testid="home-panels"]')
        expect(homePanels).not.toBeInTheDocument()
    })

    it('should not show home panels if showEnterpriseHomePanels disabled', () => {
        container = render(<SearchPage {...defaultProps} />).container
        const homePanels = container.querySelector('[data-testid="home-panels"]')
        expect(homePanels).not.toBeInTheDocument()
    })

    it('should show home panels if showEnterpriseHomePanels enabled and not on Sourcegraph.com', () => {
        useExperimentalFeatures.setState({ showEnterpriseHomePanels: true })

        container = render(<SearchPage {...defaultProps} />).container
        const homePanels = container.querySelector('[data-testid="home-panels"]')
        expect(homePanels).toBeVisible()
    })
})
