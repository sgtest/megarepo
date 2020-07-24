import React from 'react'
import renderer from 'react-test-renderer'
import { setLinkComponent } from '../../../shared/src/components/Link'
import * as GQL from '../../../shared/src/graphql/schema'
import { ThemePreference } from '../theme'
import { GlobalNavbar } from './GlobalNavbar'
import { createLocation, createMemoryHistory } from 'history'
import { NOOP_SETTINGS_CASCADE } from '../../../shared/src/util/searchTestHelpers'

const PROPS: GlobalNavbar['props'] = {
    authenticatedUser: null,
    extensionsController: {} as any,
    location: createLocation('/'),
    history: createMemoryHistory(),
    keyboardShortcuts: [],
    isSourcegraphDotCom: false,
    navbarSearchQueryState: { query: 'q', cursorPosition: 0 },
    onNavbarQueryChange: () => undefined,
    onThemePreferenceChange: () => undefined,
    isLightTheme: true,
    themePreference: ThemePreference.Light,
    patternType: GQL.SearchPatternType.literal,
    setPatternType: () => undefined,
    caseSensitive: false,
    setCaseSensitivity: () => undefined,
    platformContext: {} as any,
    settingsCascade: NOOP_SETTINGS_CASCADE,
    showCampaigns: false,
    telemetryService: {} as any,
    hideNavLinks: true, // used because reactstrap Popover is incompatible with react-test-renderer
    filtersInQuery: {} as any,
    splitSearchModes: false,
    interactiveSearchMode: false,
    toggleSearchMode: () => undefined,
    onFiltersInQueryChange: () => undefined,
    smartSearchField: false,
    isSearchRelatedPage: true,
    copyQueryButton: false,
    versionContext: undefined,
    setVersionContext: () => undefined,
    availableVersionContexts: [],
    variant: 'default',
    globbing: false,
}

describe('GlobalNavbar', () => {
    setLinkComponent(props => <a {...props} />)
    afterAll(() => setLinkComponent(() => null)) // reset global env for other tests

    test('default', () => expect(renderer.create(<GlobalNavbar {...PROPS} />).toJSON()).toMatchSnapshot())

    test('low-profile', () =>
        expect(renderer.create(<GlobalNavbar {...PROPS} variant="low-profile" />).toJSON()).toMatchSnapshot())

    test('low-profile-with-logo', () =>
        expect(renderer.create(<GlobalNavbar {...PROPS} variant="low-profile-with-logo" />).toJSON()).toMatchSnapshot())

    test('no-search-input', () =>
        expect(renderer.create(<GlobalNavbar {...PROPS} variant="no-search-input" />).toJSON()).toMatchSnapshot())
})
