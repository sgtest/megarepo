import { storiesOf } from '@storybook/react'
import { subDays } from 'date-fns'
import { Observable, of } from 'rxjs'

import { ListSearchContextsResult } from '@sourcegraph/search'
import {
    mockFetchAutoDefinedSearchContexts,
    mockFetchSearchContexts,
    mockGetUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/testing/searchContexts/testHelpers'
import { NOOP_PLATFORM_CONTEXT } from '@sourcegraph/shared/src/testing/searchTestHelpers'

import { WebStory } from '../../components/WebStory'

import { SearchContextsListTab, SearchContextsListTabProps } from './SearchContextsListTab'

const { add } = storiesOf('web/enterprise/searchContexts/SearchContextsListTab', module)
    .addParameters({
        chromatic: { viewports: [1200], disableSnapshot: false },
    })
    .addDecorator(story => (
        <div className="p-3 container" style={{ position: 'static' }}>
            {story()}
        </div>
    ))

const defaultProps: SearchContextsListTabProps = {
    authenticatedUser: null,
    isSourcegraphDotCom: true,
    fetchAutoDefinedSearchContexts: mockFetchAutoDefinedSearchContexts(),
    fetchSearchContexts: mockFetchSearchContexts,
    getUserSearchContextNamespaces: mockGetUserSearchContextNamespaces,
    platformContext: NOOP_PLATFORM_CONTEXT,
}

const propsWithContexts: SearchContextsListTabProps = {
    ...defaultProps,
    fetchAutoDefinedSearchContexts: mockFetchAutoDefinedSearchContexts(1),
    fetchSearchContexts: ({
        first,
        query,
        after,
    }: {
        first: number
        query?: string
        after?: string
    }): Observable<ListSearchContextsResult['searchContexts']> =>
        of({
            nodes: [
                {
                    __typename: 'SearchContext',
                    id: '3',
                    spec: '@username/test-version-1.5',
                    name: 'test-version-1.5',
                    namespace: {
                        __typename: 'User',
                        id: 'u1',
                        namespaceName: 'username',
                    },
                    autoDefined: false,
                    public: true,
                    description: 'Only code in version 1.5',
                    query: '',
                    updatedAt: subDays(new Date(), 1).toISOString(),
                    repositories: [],
                    viewerCanManage: true,
                },
                {
                    __typename: 'SearchContext',
                    id: '4',
                    spec: '@username/test-version-1.6',
                    namespace: {
                        __typename: 'User',
                        id: 'u1',
                        namespaceName: 'username',
                    },
                    name: 'test-version-1.6',
                    autoDefined: false,
                    public: false,
                    description: 'Only code in version 1.6',
                    query: '',
                    updatedAt: subDays(new Date(), 1).toISOString(),
                    repositories: [],
                    viewerCanManage: true,
                },
            ],
            pageInfo: {
                endCursor: null,
                hasNextPage: false,
            },
            totalCount: 1,
        }),
}

add('default', () => <WebStory>{() => <SearchContextsListTab {...defaultProps} />}</WebStory>, {})

add(
    'with SourcegraphDotCom disabled',
    () => <WebStory>{() => <SearchContextsListTab {...propsWithContexts} isSourcegraphDotCom={false} />}</WebStory>,
    {}
)

add(
    'with 1 auto-defined context',
    () => <WebStory>{() => <SearchContextsListTab {...propsWithContexts} />}</WebStory>,
    {}
)

add(
    'with 2 auto-defined contexts',
    () => (
        <WebStory>
            {() => (
                <SearchContextsListTab
                    {...propsWithContexts}
                    fetchAutoDefinedSearchContexts={mockFetchAutoDefinedSearchContexts(2)}
                />
            )}
        </WebStory>
    ),
    {}
)

add(
    'with 3 auto-defined contexts',
    () => (
        <WebStory>
            {() => (
                <SearchContextsListTab
                    {...propsWithContexts}
                    fetchAutoDefinedSearchContexts={mockFetchAutoDefinedSearchContexts(3)}
                />
            )}
        </WebStory>
    ),
    {}
)

add(
    'with 4 auto-defined contexts',
    () => (
        <WebStory>
            {() => (
                <SearchContextsListTab
                    {...propsWithContexts}
                    fetchAutoDefinedSearchContexts={mockFetchAutoDefinedSearchContexts(4)}
                />
            )}
        </WebStory>
    ),
    {}
)
