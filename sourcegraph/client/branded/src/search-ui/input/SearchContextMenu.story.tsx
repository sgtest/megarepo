import { Meta, Story, DecoratorFn } from '@storybook/react'
import { Observable, of } from 'rxjs'

import { ListSearchContextsResult } from '@sourcegraph/shared/src/graphql-operations'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    mockFetchSearchContexts,
    mockGetUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/testing/searchContexts/testHelpers'
import { NOOP_PLATFORM_CONTEXT } from '@sourcegraph/shared/src/testing/searchTestHelpers'
import { BrandedStory } from '@sourcegraph/wildcard/src/stories'

import { SearchContextMenu, SearchContextMenuProps } from './SearchContextMenu'

const decorator: DecoratorFn = story => (
    <div className="dropdown-menu show" style={{ position: 'static' }}>
        {story()}
    </div>
)

const config: Meta = {
    title: 'branded/search-ui/input/SearchContextMenu',
    parameters: {
        chromatic: { viewports: [500], disableSnapshot: false },
        design: {
            type: 'figma',
            url: 'https://www.figma.com/file/4Fy9rURbfF2bsl4BvYunUO/RFC-261-Search-Contexts?node-id=581%3A4754',
        },
    },
    decorators: [decorator],
}

export default config

const defaultProps: SearchContextMenuProps = {
    authenticatedUser: null,
    isSourcegraphDotCom: false,
    showSearchContextManagement: false,
    fetchSearchContexts: mockFetchSearchContexts,
    selectedSearchContextSpec: 'global',
    selectSearchContextSpec: () => {},
    onMenuClose: () => {},
    getUserSearchContextNamespaces: mockGetUserSearchContextNamespaces,
    searchContextsEnabled: true,
    platformContext: NOOP_PLATFORM_CONTEXT,
    telemetryService: NOOP_TELEMETRY_SERVICE,
}

const emptySearchContexts = {
    fetchSearchContexts: (): Observable<ListSearchContextsResult['searchContexts']> =>
        of({
            nodes: [],
            pageInfo: {
                endCursor: null,
                hasNextPage: false,
            },
            totalCount: 0,
        }),
}

export const Default: Story = () => <BrandedStory>{() => <SearchContextMenu {...defaultProps} />}</BrandedStory>

export const Empty: Story = () => (
    <BrandedStory>{() => <SearchContextMenu {...defaultProps} {...emptySearchContexts} />}</BrandedStory>
)

export const WithManageLink: Story = () => (
    <BrandedStory>{() => <SearchContextMenu {...defaultProps} showSearchContextManagement={true} />}</BrandedStory>
)

WithManageLink.storyName = 'with manage link'

export const WithCTALink: Story = () => (
    <BrandedStory>
        {() => <SearchContextMenu {...defaultProps} showSearchContextManagement={true} isSourcegraphDotCom={true} />}
    </BrandedStory>
)

WithCTALink.storyName = 'with CTA link'
