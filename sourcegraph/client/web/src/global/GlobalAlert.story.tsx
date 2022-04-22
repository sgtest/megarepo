import { Meta, Story } from '@storybook/react'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { AlertType } from '../graphql-operations'

import { GlobalAlert } from './GlobalAlert'

const config: Meta = {
    title: 'web/GlobalAlert',

    decorators: [
        story => (
            <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
        ),
    ],

    parameters: {
        component: GlobalAlert,
        chromatic: {
            disableSnapshot: false,
        },
    },
}

export default config

export const GlobalAlerts: Story = () => (
    <div>
        <h1>Global Alert</h1>
        <p>
            These alerts map to the <code>AlertType</code> returned from the backend API
        </p>
        <h2>Variants</h2>
        {Object.values(AlertType).map(type => (
            <GlobalAlert key={type} alert={{ message: 'Something happened!', isDismissibleWithKey: null, type }} />
        ))}
        <h2>Dismissible</h2>
        <GlobalAlert
            alert={{ message: 'You can dismiss me', isDismissibleWithKey: 'dismiss-key', type: AlertType.INFO }}
        />
    </div>
)
