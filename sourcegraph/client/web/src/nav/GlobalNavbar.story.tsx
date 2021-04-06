import React from 'react'
import { createMemoryHistory } from 'history'
import { storiesOf } from '@storybook/react'
import { WebStory } from '../components/WebStory'
import { GlobalNavbar } from './GlobalNavbar'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemePreference } from '../theme'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { SearchPatternType } from '../graphql-operations'
import { SourcegraphContext } from '../jscontext'
import { SuiteFunction } from 'mocha'
import { AuthenticatedUser } from '../auth'
import { extensionsController } from '@sourcegraph/shared/src/util/searchTestHelpers'
import { mockFetchAutoDefinedSearchContexts, mockFetchSearchContexts } from '../searchContexts/testHelpers'

window.context = { assetsRoot: 'https://sourcegraph.com/.assets' } as SourcegraphContext & SuiteFunction

const history = createMemoryHistory()

const defaultProps = (
    props: ThemeProps
): Omit<
    React.ComponentProps<typeof GlobalNavbar>,
    'authenticatedUser' | 'variant' | 'isSearchRelatedPage' | 'authRequired'
> => ({
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
    setVersionContext: () => Promise.resolve(undefined),
    availableVersionContexts: [],
    globbing: false,
    enableSmartQuery: false,
    parsedSearchQuery: 'r:golang/oauth2 test f:travis',
    patternType: SearchPatternType.literal,
    setPatternType: () => undefined,
    caseSensitive: false,
    setCaseSensitivity: () => undefined,
    platformContext: {} as any,
    keyboardShortcuts: [],
    copyQueryButton: false,
    versionContext: undefined,
    showSearchContext: false,
    selectedSearchContextSpec: '',
    setSelectedSearchContextSpec: () => undefined,
    defaultSearchContextSpec: '',
    showOnboardingTour: false,
    isLightTheme: props.isLightTheme,
    navbarSearchQueryState: { query: '' },
    onNavbarQueryChange: () => {},
    isExtensionAlertAnimating: false,
    showBatchChanges: true,
    enableCodeMonitoring: true,
    activation: undefined,
    hideNavLinks: false,
    routes: [],
    fetchAutoDefinedSearchContexts: mockFetchAutoDefinedSearchContexts(),
    fetchSearchContexts: mockFetchSearchContexts,
})

const { add } = storiesOf('web/nav/GlobalNav', module)

add('Anonymous viewer', () => (
    <WebStory>
        {webProps => (
            <GlobalNavbar
                {...defaultProps(webProps)}
                authRequired={false}
                authenticatedUser={null}
                variant="default"
                isSearchRelatedPage={false}
            />
        )}
    </WebStory>
))

add('Auth required', () => (
    <WebStory>
        {webProps => (
            <GlobalNavbar
                {...defaultProps(webProps)}
                authRequired={true}
                authenticatedUser={null}
                variant="default"
                isSearchRelatedPage={false}
            />
        )}
    </WebStory>
))

add(
    'Authenticated viewer',
    () => (
        <WebStory>
            {webProps => (
                <GlobalNavbar
                    {...defaultProps(webProps)}
                    authRequired={false}
                    authenticatedUser={
                        { username: 'alice', organizations: { nodes: [{ name: 'acme' }] } } as AuthenticatedUser
                    }
                    variant="default"
                    isSearchRelatedPage={false}
                />
            )}
        </WebStory>
    ),
    {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/SFhXbl23TJ2j5tOF51NDtF/%F0%9F%93%9AWeb?node-id=985%3A1281',
        },
    }
)
