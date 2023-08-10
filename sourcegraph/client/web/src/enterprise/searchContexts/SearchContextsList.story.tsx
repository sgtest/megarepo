import type { DecoratorFn, Meta, Story } from '@storybook/react'

import {
    mockAuthenticatedUser,
    mockFetchSearchContexts,
    mockGetUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/testing/searchContexts/testHelpers'
import { NOOP_PLATFORM_CONTEXT } from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { WebStory } from '../../components/WebStory'

import { SearchContextsList, type SearchContextsListProps } from './SearchContextsList'

const decorator: DecoratorFn = story => (
    <div className="p-3 container" style={{ position: 'static' }}>
        {story()}
    </div>
)

const config: Meta = {
    title: 'web/enterprise/searchContexts/SearchContextsListTab',
    decorators: [decorator],
    parameters: {
        chromatic: { viewports: [1200, 767], disableSnapshot: false },
    },
}

export default config

const defaultProps: SearchContextsListProps = {
    authenticatedUser: mockAuthenticatedUser,
    fetchSearchContexts: mockFetchSearchContexts,
    getUserSearchContextNamespaces: mockGetUserSearchContextNamespaces,
    platformContext: NOOP_PLATFORM_CONTEXT,
    setAlert: () => undefined,
}

export const Default: Story = () => <WebStory>{() => <SearchContextsList {...defaultProps} />}</WebStory>
