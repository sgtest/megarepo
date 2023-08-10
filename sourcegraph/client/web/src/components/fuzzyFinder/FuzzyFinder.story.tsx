import type { DecoratorFn, Meta, Story } from '@storybook/react'

import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import { WebStory } from '../WebStory'

import { FuzzyWrapper, FUZZY_FILES_MOCK } from './FuzzyFinder.mocks'

const decorator: DecoratorFn = story => <WebStory>{() => story()}</WebStory>

const config: Meta = {
    title: 'web/FuzzyFinder',
    decorators: [decorator],
    parameters: {
        chromatic: {
            disableSnapshot: false,
        },
    },
}

export default config

export const ReadyStory: Story = () => (
    <MockedTestProvider mocks={[FUZZY_FILES_MOCK]}>
        <FuzzyWrapper url="/github.com/sourcegraph/sourcegraph@main" experimentalFeatures={{}} initialQuery="clientb" />
    </MockedTestProvider>
)

export const ReadyFileLineStory: Story = () => (
    <MockedTestProvider mocks={[FUZZY_FILES_MOCK]}>
        <FuzzyWrapper
            url="/github.com/sourcegraph/sourcegraph@main"
            experimentalFeatures={{}}
            activeTab="files"
            initialQuery="clientb:100"
        />
    </MockedTestProvider>
)

export const TabsStory: Story = () => (
    <MockedTestProvider mocks={[FUZZY_FILES_MOCK]}>
        <FuzzyWrapper
            url="/github.com/sourcegraph/sourcegraph@main"
            experimentalFeatures={{ fuzzyFinderAll: true }}
            initialQuery="clientb"
        />
    </MockedTestProvider>
)
