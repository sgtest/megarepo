import { mdiClose } from '@mdi/js'
import { Story, Meta } from '@storybook/react'
import CloseIcon from 'mdi-react/CloseIcon'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { Icon } from '..'
import { H3 } from '../..'
import { SourcegraphIcon } from '../SourcegraphIcon'
import { Code } from '../Typography'

const config: Meta = {
    title: 'wildcard/Icon',

    decorators: [
        story => (
            <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
        ),
    ],

    parameters: {
        component: Icon,
        chromatic: {
            enableDarkMode: true,
            disableSnapshot: false,
        },
        design: {
            type: 'figma',
            name: 'Figma',
            url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Wildcard-Design-System?node-id=1366%3A611',
        },
    },
}
export default config

export const Simple: Story = () => (
    <>
        <H3>Small Icon</H3>
        <Icon as={SourcegraphIcon} size="sm" aria-label="Sourcegraph logo" />

        <H3>Medium Icon</H3>
        <Icon as={SourcegraphIcon} size="md" aria-label="Sourcegraph logo" />

        <H3>
            Legacy <Code>mdi-react</Code> Icon
        </H3>
        <Icon as={CloseIcon} size="md" aria-label="Close" />

        <H3>
            New <Code>@mdi/js</Code> Icon
        </H3>
        <Icon svgPath={mdiClose} size="md" aria-label="Close" />
    </>
)
