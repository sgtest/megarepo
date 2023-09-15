import { MockedResponse } from '@apollo/client/testing'
import { Meta, Story } from '@storybook/react'

import { getDocumentNode } from '@sourcegraph/http-client'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import { WebStory } from '../../components/WebStory'
import {
    GetUsersListResult,
    GetUsersListVariables,
    SearchJobsOrderBy,
    SearchJobsResult,
    SearchJobState,
    SearchJobsVariables,
} from '../../graphql-operations'

import { SEARCH_JOBS_QUERY, SearchJobsPage } from './SearchJobsPage'
import { GET_USERS_QUERY } from './UsersPicker'

const defaultStory: Meta = {
    title: 'web/search-jobs',
    decorators: [story => <WebStory>{() => story()}</WebStory>],
    parameters: {
        chromatic: {
            disableSnapshot: false,
        },
    },
}

export default defaultStory

const SEARCH_JOBS_MOCK: MockedResponse<SearchJobsResult, SearchJobsVariables> = {
    request: {
        query: getDocumentNode(SEARCH_JOBS_QUERY),
        variables: {
            first: 20,
            after: null,
            query: '',
            states: [],
            orderBy: SearchJobsOrderBy.CREATED_DATE,
        },
    },
    result: {
        data: {
            __typename: 'Query',
            searchJobs: {
                __typename: 'SearchJobConnection',
                nodes: [
                    {
                        __typename: 'SearchJob',
                        id: '001',
                        finishedAt: null,
                        startedAt: '2023-09-12T20:42:46Z',
                        state: SearchJobState.QUEUED,
                        query: 'repo:sourcegraph/* insights rev:asdf',
                        URL: null,
                        repoStats: {
                            __typename: 'SearchJobStats',
                            total: 200,
                            completed: 0,
                            failed: 0,
                            inProgress: 0,
                        },
                        creator: {
                            __typename: 'User',
                            id: 'u_001',
                            username: 'p_Kapitsa',
                            displayName: 'Pyotr Kapitsa',
                            avatarURL: null,
                        },
                    },
                    {
                        __typename: 'SearchJob',
                        id: '002',
                        finishedAt: null,
                        startedAt: '2023-09-12T20:42:46Z',
                        state: SearchJobState.PROCESSING,
                        query: 'repo:sourcegraph/* batch-changes rev:asdf',
                        URL: null,
                        repoStats: {
                            __typename: 'SearchJobStats',
                            total: 145,
                            completed: 24,
                            failed: 1,
                            inProgress: 43,
                        },
                        creator: {
                            __typename: 'User',
                            id: 'u_001',
                            username: 'p_Kapitsa',
                            displayName: 'Pyotr Kapitsa',
                            avatarURL: null,
                        },
                    },
                    {
                        __typename: 'SearchJob',
                        id: '003',
                        finishedAt: null,
                        startedAt: '2023-09-12T20:42:46Z',
                        state: SearchJobState.FAILED,
                        query: 'repo:sourcegraph/* import { Button ',
                        URL: null,
                        repoStats: {
                            __typename: 'SearchJobStats',
                            total: 155,
                            completed: 24,
                            failed: 4,
                            inProgress: 43,
                        },
                        creator: {
                            __typename: 'User',
                            id: 'u_001',
                            username: 'p_Kapitsa',
                            displayName: 'Pyotr Kapitsa',
                            avatarURL: null,
                        },
                    },
                    {
                        __typename: 'SearchJob',
                        id: '004',
                        finishedAt: null,
                        startedAt: '2023-08-23',
                        state: SearchJobState.ERRORED,
                        query: 'repo:sourcegraph/* import { Button ',
                        URL: null,
                        repoStats: {
                            __typename: 'SearchJobStats',
                            total: 155,
                            completed: 24,
                            failed: 4,
                            inProgress: 43,
                        },
                        creator: {
                            __typename: 'User',
                            id: 'u_001',
                            username: 'p_Kapitsa',
                            displayName: 'Pyotr Kapitsa',
                            avatarURL: null,
                        },
                    },
                    {
                        __typename: 'SearchJob',
                        id: '005',
                        finishedAt: null,
                        startedAt: '2023-08-23',
                        state: SearchJobState.COMPLETED,
                        query: 'repo:sourcegraph/* import { Button ',
                        URL: null,
                        repoStats: {
                            __typename: 'SearchJobStats',
                            total: 155,
                            completed: 24,
                            failed: 4,
                            inProgress: 43,
                        },
                        creator: {
                            __typename: 'User',
                            id: 'u_001',
                            username: 'p_Kapitsa',
                            displayName: 'Pyotr Kapitsa',
                            avatarURL: null,
                        },
                    },
                ],
                totalCount: 5,
                pageInfo: {
                    __typename: 'PageInfo',
                    hasNextPage: false,
                    endCursor: null,
                },
            },
        },
    },
}

const USER_PICKER_QUERY_MOCK: MockedResponse<GetUsersListResult, GetUsersListVariables> = {
    request: {
        query: getDocumentNode(GET_USERS_QUERY),
        variables: {
            query: '',
        },
    },
    result: {
        data: {
            __typename: 'Query',
            users: {
                __typename: 'UserConnection',
                nodes: [
                    {
                        __typename: 'User',
                        id: 'user_001',
                        username: 'pyotr_kapica',
                        displayName: 'Pyotr Kapitsa',
                        avatarURL: null,
                        siteAdmin: true,
                        primaryEmail: {
                            __typename: 'UserEmail',
                            email: 'pyotrkapica@сambridge.com',
                        },
                    },
                    {
                        __typename: 'User',
                        id: 'user_002',
                        username: 'lev_landau',
                        displayName: 'Lev Landau',
                        avatarURL: null,
                        siteAdmin: false,
                        primaryEmail: {
                            __typename: 'UserEmail',
                            email: 'levlandau@bdu.com',
                        },
                    },
                    {
                        __typename: 'User',
                        id: 'user_003',
                        username: 'alex_shalnikov',
                        displayName: 'Alexandr Shalnikov',
                        avatarURL: null,
                        siteAdmin: false,
                        primaryEmail: {
                            __typename: 'UserEmail',
                            email: 'alexshalnikov@spbstu.com',
                        },
                    },
                    {
                        __typename: 'User',
                        id: 'user_004',
                        username: 'yuri_kondratyuk',
                        displayName: 'Yuri Kondratyuk',
                        avatarURL: null,
                        siteAdmin: false,
                        primaryEmail: {
                            __typename: 'UserEmail',
                            email: 'yurikondratyuk@mail.com',
                        },
                    },
                    {
                        __typename: 'User',
                        id: 'user_005',
                        username: 'alexei_abrikosov',
                        displayName: 'Alexei Abrikosov',
                        avatarURL: null,
                        siteAdmin: false,
                        primaryEmail: {
                            __typename: 'UserEmail',
                            email: 'alexeiabrikos@msu.com',
                        },
                    },
                ],
                totalCount: 5,
                pageInfo: {
                    __typename: 'PageInfo',
                    hasNextPage: false,
                    endCursor: null,
                },
            },
        },
    },
}

export const SearchJobsListPage: Story = () => (
    <MockedTestProvider mocks={[SEARCH_JOBS_MOCK, USER_PICKER_QUERY_MOCK]}>
        <SearchJobsPage isAdmin={false} />
    </MockedTestProvider>
)
