import { MockedResponse } from '@apollo/client/testing'
import { fireEvent } from '@testing-library/react'

import { dataOrThrowErrors, getDocumentNode, gql } from '@sourcegraph/http-client'
import { MockedTestProvider, waitForNextApolloResponse } from '@sourcegraph/shared/src/testing/apollo'
import { Text } from '@sourcegraph/wildcard'
import { RenderWithBrandedContextResult, renderWithBrandedContext } from '@sourcegraph/wildcard/src/testing'

import {
    TestShowMorePaginationQueryFields,
    TestShowMorePaginationQueryResult,
    TestShowMorePaginationQueryVariables,
} from '../../../graphql-operations'

import { useShowMorePagination } from './useShowMorePagination'

const TEST_SHOW_MORE_PAGINATION_QUERY = gql`
    query TestShowMorePaginationQuery($first: Int) {
        repositories(first: $first) {
            nodes {
                id
                name
            }
            totalCount
            pageInfo {
                endCursor
                hasNextPage
            }
        }
    }

    fragment TestShowMorePaginationQueryFields on Repository {
        name
        id
    }
`

const TestComponent = ({ skip = false }) => {
    const { connection, fetchMore, hasNextPage } = useShowMorePagination<
        TestShowMorePaginationQueryResult,
        TestShowMorePaginationQueryVariables,
        TestShowMorePaginationQueryFields
    >({
        query: TEST_SHOW_MORE_PAGINATION_QUERY,
        variables: {
            first: 1,
        },
        getConnection: result => {
            const data = dataOrThrowErrors(result)
            return data.repositories
        },
        options: {
            useURL: true,
            skip,
        },
    })

    return (
        <>
            <ul>
                {connection?.nodes.map((node, index) => (
                    <li key={index}>{node.name}</li>
                ))}
            </ul>

            {connection?.totalCount && <Text>Total count: {connection.totalCount}</Text>}
            {hasNextPage && (
                <button type="button" onClick={fetchMore}>
                    Fetch more
                </button>
            )}
        </>
    )
}

const generateMockRequest = ({
    after,
    first,
}: {
    after?: string
    first: number
}): MockedResponse<TestShowMorePaginationQueryResult>['request'] => ({
    query: getDocumentNode(TEST_SHOW_MORE_PAGINATION_QUERY),
    variables: {
        after,
        first,
    },
})

const generateMockResult = ({
    endCursor,
    hasNextPage,
    nodes,
    totalCount,
}: {
    endCursor: string | null
    hasNextPage: boolean
    nodes: TestShowMorePaginationQueryFields[]
    totalCount: number
}): MockedResponse<TestShowMorePaginationQueryResult>['result'] => ({
    data: {
        repositories: {
            nodes,
            pageInfo: {
                endCursor,
                hasNextPage,
            },
            totalCount,
        },
    },
})

describe('useShowMorePagination', () => {
    const fetchNextPage = async (renderResult: RenderWithBrandedContextResult) => {
        const fetchMoreButton = renderResult.getByText('Fetch more')
        fireEvent.click(fetchMoreButton)

        // Skip loading state
        await waitForNextApolloResponse()
    }

    const mockResultNodes: TestShowMorePaginationQueryFields[] = [
        {
            id: 'A',
            name: 'repo-A',
        },
        {
            id: 'B',
            name: 'repo-B',
        },
        {
            id: 'C',
            name: 'repo-C',
        },
        {
            id: 'D',
            name: 'repo-D',
        },
    ]

    const renderWithMocks = async (
        mocks: MockedResponse<TestShowMorePaginationQueryResult>[],
        route = '/',
        skip = false
    ) => {
        const renderResult = renderWithBrandedContext(
            <MockedTestProvider mocks={mocks}>
                <TestComponent skip={skip} />
            </MockedTestProvider>,
            { route }
        )

        // Skip loading state
        await waitForNextApolloResponse()

        return renderResult
    }

    describe('Cursor based pagination', () => {
        const generateMockCursorResponses = (
            nodes: TestShowMorePaginationQueryFields[]
        ): MockedResponse<TestShowMorePaginationQueryResult>[] =>
            nodes.map((node, index) => {
                const isFirstPage = index === 0
                const cursor = !isFirstPage ? String(index) : undefined
                return {
                    request: generateMockRequest({ after: cursor, first: 1 }),
                    result: generateMockResult({
                        nodes: [node],
                        endCursor: String(index + 1),
                        hasNextPage: index !== nodes.length - 1,
                        totalCount: nodes.length,
                    }),
                }
            })

        const cursorMocks = generateMockCursorResponses(mockResultNodes)

        it('does not fetch anything if skip is true', async () => {
            const queries = await renderWithMocks(cursorMocks, '/', true)
            expect(queries.queryByText('repo-A')).not.toBeInTheDocument()
            expect(queries.queryByText('repo-C')).not.toBeInTheDocument()
            expect(queries.queryByText('repo-D')).not.toBeInTheDocument()
            expect(queries.queryByText('Total count')).not.toBeInTheDocument()
        })

        it('renders correct result', async () => {
            const queries = await renderWithMocks(cursorMocks)
            expect(queries.getAllByRole('listitem').length).toBe(1)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()
            expect(queries.getByText('Fetch more')).toBeVisible()
            expect(queries.locationRef.current?.search).toBe('')
        })

        it('fetches next page of results correctly', async () => {
            const queries = await renderWithMocks(cursorMocks)

            // Fetch first page
            await fetchNextPage(queries)

            // Both pages are now displayed
            expect(queries.getAllByRole('listitem').length).toBe(2)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('repo-B')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()
            expect(queries.getByText('Fetch more')).toBeVisible()

            // URL updates to match visible results
            expect(queries.locationRef.current?.search).toBe('?visible=2')
        })

        it('fetches final page of results correctly', async () => {
            const queries = await renderWithMocks(cursorMocks)

            // Fetch all pages
            await fetchNextPage(queries)
            await fetchNextPage(queries)
            await fetchNextPage(queries)

            // All pages of results are displayed
            expect(queries.getAllByRole('listitem').length).toBe(4)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('repo-B')).toBeVisible()
            expect(queries.getByText('repo-C')).toBeVisible()
            expect(queries.getByText('repo-D')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()

            // Fetch more button is now no longer visible
            expect(queries.queryByText('Fetch more')).not.toBeInTheDocument()

            // URL updates to match visible results
            expect(queries.locationRef.current?.search).toBe('?visible=4')
        })

        it('fetches correct amount of results when navigating directly with a URL', async () => {
            // We need to add an extra mock here, as we will derive a different `first` variable from `visible` in the URL.
            const mockFromVisible: MockedResponse<TestShowMorePaginationQueryResult> = {
                request: generateMockRequest({ first: 3 }),
                result: generateMockResult({
                    nodes: [mockResultNodes[0], mockResultNodes[1], mockResultNodes[2]],
                    hasNextPage: true,
                    endCursor: '3',
                    totalCount: 4,
                }),
            }

            const queries = await renderWithMocks([...cursorMocks, mockFromVisible], '/?visible=3')

            // Renders 3 results without having to manually fetch
            expect(queries.getAllByRole('listitem').length).toBe(3)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('repo-B')).toBeVisible()
            expect(queries.getByText('repo-C')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()

            // Fetching next page should work as usual
            await fetchNextPage(queries)
            expect(queries.getAllByRole('listitem').length).toBe(4)
            expect(queries.getByText('repo-C')).toBeVisible()
            expect(queries.getByText('repo-D')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()

            // URL should be overidden
            expect(queries.locationRef.current?.search).toBe('?visible=4')
        })
    })

    describe('Batch based pagination', () => {
        const batchedMocks: MockedResponse<TestShowMorePaginationQueryResult>[] = [
            {
                request: generateMockRequest({ first: 1 }),
                result: generateMockResult({
                    nodes: [mockResultNodes[0]],
                    endCursor: null,
                    hasNextPage: true,
                    totalCount: 4,
                }),
            },
            {
                request: generateMockRequest({ first: 2 }),
                result: generateMockResult({
                    nodes: [mockResultNodes[0], mockResultNodes[1]],
                    endCursor: null,
                    hasNextPage: true,
                    totalCount: 4,
                }),
            },
            {
                request: generateMockRequest({ first: 4 }),
                result: generateMockResult({
                    nodes: mockResultNodes,
                    endCursor: null,
                    hasNextPage: false,
                    totalCount: 4,
                }),
            },
        ]

        it('renders correct result', async () => {
            const queries = await renderWithMocks(batchedMocks)
            expect(queries.getAllByRole('listitem').length).toBe(1)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()
            expect(queries.getByText('Fetch more')).toBeVisible()
            expect(queries.locationRef.current?.search).toBe('')
        })

        it('fetches next page of results correctly', async () => {
            const queries = await renderWithMocks(batchedMocks)

            // Fetch first page
            await fetchNextPage(queries)

            // Both pages are now displayed
            expect(queries.getAllByRole('listitem').length).toBe(2)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('repo-B')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()
            expect(queries.getByText('Fetch more')).toBeVisible()

            // URL updates to match the new request
            expect(queries.locationRef.current?.search).toBe('?first=2')
        })

        it('fetches final page of results correctly', async () => {
            const queries = await renderWithMocks(batchedMocks)

            // Fetch both pages
            await fetchNextPage(queries)
            await fetchNextPage(queries)

            // All pages of results are displayed
            expect(queries.getAllByRole('listitem').length).toBe(4)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('repo-B')).toBeVisible()
            expect(queries.getByText('repo-C')).toBeVisible()
            expect(queries.getByText('repo-D')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()

            // Fetch more button is now no longer visible
            expect(queries.queryByText('Fetch more')).not.toBeInTheDocument()

            // URL updates to match the new request
            expect(queries.locationRef.current?.search).toBe('?first=4')
        })

        it('fetches correct amount of results when navigating directly with a URL', async () => {
            const queries = await renderWithMocks(batchedMocks, '/?first=2')

            // Renders 2 results without having to manually fetch
            expect(queries.getAllByRole('listitem').length).toBe(2)
            expect(queries.getByText('repo-A')).toBeVisible()
            expect(queries.getByText('repo-B')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()

            // Fetching next page should work as usual
            await fetchNextPage(queries)
            expect(queries.getAllByRole('listitem').length).toBe(4)
            expect(queries.getByText('repo-C')).toBeVisible()
            expect(queries.getByText('repo-D')).toBeVisible()
            expect(queries.getByText('Total count: 4')).toBeVisible()

            // URL should be overidden
            expect(queries.locationRef.current?.search).toBe('?first=4')
        })
    })
})
