import { Meta, Story } from '@storybook/react'

import { BrandedStory } from '@sourcegraph/wildcard/src/stories'

import { AfterInstallPageContent } from './AfterInstallPageContent'

import brandedStyles from '../../branded.scss'

const config: Meta = {
    title: 'browser/AfterInstallPage',
    parameters: {
        chromatic: {
            enableDarkMode: true,
            disableSnapshot: false,
        },
    },
}

export default config

export const Default: Story = () => <BrandedStory styles={brandedStyles}>{AfterInstallPageContent}</BrandedStory>
