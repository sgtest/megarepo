import { storiesOf } from '@storybook/react'
import React from 'react'
import { of } from 'rxjs'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { ExternalServiceKind } from '../../graphql-operations'
import { WebStory } from '../WebStory'

import { queryExternalServices as _queryExternalServices } from './backend'
import { ExternalServicesPage } from './ExternalServicesPage'

const { add } = storiesOf('web/External services/ExternalServicesPage', module).addDecorator(story => (
    <div className="p-3 container">{story()}</div>
))

const queryExternalServices: typeof _queryExternalServices = () =>
    of({
        totalCount: 1,
        pageInfo: {
            endCursor: null,
            hasNextPage: false,
        },
        nodes: [
            {
                id: 'service1',
                kind: ExternalServiceKind.GITHUB,
                displayName: 'GitHub.com',
                config: '{"githubconfig":true}',
                warning: null,
                lastSyncError: null,
                repoCount: 0,
                lastSyncAt: '0001-01-01T00:00:00Z',
                nextSyncAt: '0001-01-01T00:00:00Z',
                updatedAt: '2021-03-15T19:39:11Z',
                createdAt: '2021-03-15T19:39:11Z',
            },
        ],
    })

add('List of external services', () => (
    <WebStory>
        {webProps => (
            <ExternalServicesPage
                {...webProps}
                routingPrefix="/site-admin"
                afterDeleteRoute="/site-admin/after"
                telemetryService={NOOP_TELEMETRY_SERVICE}
                authenticatedUser={{ id: '123' }}
                queryExternalServices={queryExternalServices}
            />
        )}
    </WebStory>
))
