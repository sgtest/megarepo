import { Meta, Story } from '@storybook/react'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    mockFetchSearchContexts,
    mockGetUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/testing/searchContexts/testHelpers'

import { WebStory } from '../../components/WebStory'
import { MockedFeatureFlagsProvider } from '../../featureFlags/MockedFeatureFlagsProvider'

import { SearchPage, SearchPageProps } from './SearchPage'

const defaultProps: SearchPageProps = {
    isSourcegraphDotCom: false,
    settingsCascade: {
        final: null,
        subjects: null,
    },
    telemetryService: NOOP_TELEMETRY_SERVICE,
    authenticatedUser: null,
    globbing: false,
    platformContext: {} as any,
    searchContextsEnabled: true,
    selectedSearchContextSpec: '',
    setSelectedSearchContextSpec: () => {},
    fetchSearchContexts: mockFetchSearchContexts,
    getUserSearchContextNamespaces: mockGetUserSearchContextNamespaces,
}

window.context.allowSignup = true

const config: Meta = {
    title: 'web/search/home/SearchPage',
    parameters: {
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/sPRyyv3nt5h0284nqEuAXE/12192-Sourcegraph-server-page-v1?node-id=255%3A3',
        },
        chromatic: { viewports: [544, 577, 769, 993], disableSnapshot: false },
    },
}

export default config
export const CloudAuthedHome: Story = () => (
    <WebStory>{() => <SearchPage {...defaultProps} isSourcegraphDotCom={true} />}</WebStory>
)

CloudAuthedHome.storyName = 'Cloud authenticated home'

export const ServerHome: Story = () => <WebStory>{() => <SearchPage {...defaultProps} />}</WebStory>

ServerHome.storyName = 'Server home'

export const CloudMarketingHome: Story = () => (
    <WebStory>
        {() => (
            <MockedFeatureFlagsProvider overrides={{}}>
                <SearchPage {...defaultProps} isSourcegraphDotCom={true} authenticatedUser={null} />
            </MockedFeatureFlagsProvider>
        )}
    </WebStory>
)

CloudMarketingHome.storyName = 'Cloud marketing home'
