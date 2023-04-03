import { DecoratorFn, Story, Meta } from '@storybook/react'
import { subMinutes } from 'date-fns'
import { of } from 'rxjs'
import { MATCH_ANY_PARAMETERS, WildcardMockLink } from 'wildcard-mock-link'

import { getDocumentNode } from '@sourcegraph/http-client'
import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import { ExternalServiceFields, ExternalServiceKind, ExternalServiceSyncJobState } from '../../graphql-operations'
import { WebStory, WebStoryChildrenProps } from '../WebStory'

import { FETCH_EXTERNAL_SERVICE, queryExternalServiceSyncJobs as _queryExternalServiceSyncJobs } from './backend'
import { ExternalServicePage } from './ExternalServicePage'

const decorator: DecoratorFn = story => (
    <div className="p-3 container">
        <WebStory
            path="/site-admin/external-services/:externalServiceID"
            initialEntries={['/site-admin/external-services/service123']}
        >
            {story}
        </WebStory>
    </div>
)

const config: Meta = {
    title: 'web/External services/ExternalServicePage',
    decorators: [decorator],
}

export default config

const externalService = {
    __typename: 'ExternalService' as const,
    id: 'service123',
    kind: ExternalServiceKind.GITHUB,
    warning: null,
    config: '{"githubconfig": true}',
    displayName: 'GitHub.com',
    webhookURL: null,
    lastSyncError: null,
    repoCount: 1337,
    lastSyncAt: null,
    nextSyncAt: null,
    updatedAt: '2021-03-15T19:39:11Z',
    createdAt: '2021-03-15T19:39:11Z',
    hasConnectionCheck: true,
    namespace: {
        id: 'userid',
        namespaceName: 'johndoe',
        url: '/users/johndoe',
    },
}

const queryExternalServiceSyncJobs: typeof _queryExternalServiceSyncJobs = () =>
    of({
        totalCount: 4,
        pageInfo: { endCursor: null, hasNextPage: false },
        nodes: [
            {
                __typename: 'ExternalServiceSyncJob',
                failureMessage: null,
                startedAt: subMinutes(new Date(), 25).toISOString(),
                finishedAt: null,
                id: 'SYNCJOB1',
                state: ExternalServiceSyncJobState.CANCELING,
                reposSynced: 5,
                repoSyncErrors: 0,
                reposAdded: 5,
                reposDeleted: 0,
                reposModified: 0,
                reposUnmodified: 0,
            },
            {
                __typename: 'ExternalServiceSyncJob',
                failureMessage: null,
                startedAt: subMinutes(new Date(), 25).toISOString(),
                finishedAt: null,
                id: 'SYNCJOB2',
                state: ExternalServiceSyncJobState.PROCESSING,
                reposSynced: 5,
                repoSyncErrors: 0,
                reposAdded: 5,
                reposDeleted: 0,
                reposModified: 0,
                reposUnmodified: 0,
            },
            {
                __typename: 'ExternalServiceSyncJob',
                failureMessage: 'Very bad error syncing with the code host.',
                startedAt: subMinutes(new Date(), 25).toISOString(),
                finishedAt: subMinutes(new Date(), 25).toISOString(),
                id: 'SYNCJOB3',
                state: ExternalServiceSyncJobState.FAILED,
                reposSynced: 5,
                repoSyncErrors: 0,
                reposAdded: 5,
                reposDeleted: 0,
                reposModified: 0,
                reposUnmodified: 0,
            },
            {
                __typename: 'ExternalServiceSyncJob',
                failureMessage: null,
                startedAt: subMinutes(new Date(), 25).toISOString(),
                finishedAt: subMinutes(new Date(), 25).toISOString(),
                id: 'SYNCJOB4',
                state: ExternalServiceSyncJobState.COMPLETED,
                reposSynced: 5,
                repoSyncErrors: 0,
                reposAdded: 5,
                reposDeleted: 0,
                reposModified: 0,
                reposUnmodified: 0,
            },
        ],
    })

function newFetchMock(node: { __typename: 'ExternalService' } & ExternalServiceFields): WildcardMockLink {
    return new WildcardMockLink([
        {
            request: {
                query: getDocumentNode(FETCH_EXTERNAL_SERVICE),
                variables: MATCH_ANY_PARAMETERS,
            },
            result: { data: { node } },
            nMatches: Number.POSITIVE_INFINITY,
        },
    ])
}

export const ExternalServiceWithRepos: Story<WebStoryChildrenProps> = props => (
    <MockedTestProvider link={newFetchMock(externalService)}>
        <ExternalServicePage
            queryExternalServiceSyncJobs={queryExternalServiceSyncJobs}
            afterDeleteRoute="/site-admin/after-delete"
            telemetryService={NOOP_TELEMETRY_SERVICE}
            externalServicesFromFile={false}
            allowEditExternalServicesWithFile={false}
        />
    </MockedTestProvider>
)

ExternalServiceWithRepos.storyName = 'External service with synced repos'
