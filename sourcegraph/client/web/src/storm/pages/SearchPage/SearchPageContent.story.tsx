import { Meta, Story } from '@storybook/react'

import { AuthenticatedUser } from '@sourcegraph/shared/src/auth'

import { WebStory } from '../../../components/WebStory'

import { SearchPageContent } from './SearchPageContent'

window.context.allowSignup = true

const config: Meta = {
    title: 'web/search/home/SearchPageContent',
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
    <WebStory
        legacyLayoutContext={{ isSourcegraphDotCom: true, authenticatedUser: { id: 'userID' } as AuthenticatedUser }}
    >
        {() => <SearchPageContent shouldShowAddCodeHostWidget={false} />}
    </WebStory>
)

CloudAuthedHome.storyName = 'Cloud authenticated home'

export const ServerHome: Story = () => (
    <WebStory>{() => <SearchPageContent shouldShowAddCodeHostWidget={false} />}</WebStory>
)

ServerHome.storyName = 'Server home'
