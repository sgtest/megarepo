import { MockedProvider, type MockedResponse } from '@apollo/client/testing'
import { getAllByRole, getByRole, queryByRole, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { spy, stub, assert } from 'sinon'
import { describe, expect, it } from 'vitest'

import { getDocumentNode } from '@sourcegraph/http-client'
import {
    mockAuthenticatedUser,
    mockFetchSearchContexts,
    mockGetUserSearchContextNamespaces,
} from '@sourcegraph/shared/src/testing/searchContexts/testHelpers'
import { NOOP_PLATFORM_CONTEXT } from '@sourcegraph/shared/src/testing/searchTestHelpers'
import { simulateMenuItemClick } from '@sourcegraph/shared/src/testing/simulateMenuItemClick'

import type { setDefaultSearchContextResult } from '../../graphql-operations'

import { SET_DEFAULT_SEARCH_CONTEXT_MUTATION } from './hooks/useDefaultContext'
import { SearchContextsList, type SearchContextsListProps } from './SearchContextsList'

describe('SearchContextsList', () => {
    const defaultProps: SearchContextsListProps = {
        authenticatedUser: mockAuthenticatedUser,
        fetchSearchContexts: mockFetchSearchContexts,
        getUserSearchContextNamespaces: mockGetUserSearchContextNamespaces,
        setAlert: stub(),
        platformContext: NOOP_PLATFORM_CONTEXT,
    }

    describe('default context', () => {
        it('renders list with default context', () => {
            const { container } = render(
                <MockedProvider>
                    <MemoryRouter>
                        <SearchContextsList {...defaultProps} />
                    </MemoryRouter>
                </MockedProvider>
            )

            const defaultRow = getByRole(
                container,
                (role, elem) => role === 'row' && queryByRole(elem as HTMLElement, 'cell', { name: /default/i })
            )
            const contextName = getByRole(defaultRow, 'link', { name: '@user/usertest' })
            expect(contextName).toBeInTheDocument()
        })

        it('saves default context and updates list', () => {
            const mockSetDefault: MockedResponse<setDefaultSearchContextResult['setDefaultSearchContext']> = {
                request: {
                    query: getDocumentNode(SET_DEFAULT_SEARCH_CONTEXT_MUTATION),
                },
                result: {
                    data: { __typename: 'EmptyResponse', alwaysNil: null },
                },
            }

            const setAlert = spy()

            const { container } = render(
                <MockedProvider mocks={[mockSetDefault]}>
                    <MemoryRouter>
                        <SearchContextsList {...defaultProps} setAlert={setAlert} />
                    </MemoryRouter>
                </MockedProvider>
            )

            // Set first context as default
            const menuButtons = getAllByRole(container, 'button', { name: 'Actions' })
            userEvent.click(menuButtons[0])

            const setDefaultButton = screen.getByRole('menuitem', { name: 'Use as default' })
            simulateMenuItemClick(setDefaultButton)

            assert.calledOnceWithExactly(setAlert, '')

            const defaultRow = getByRole(
                container,
                (role, elem) => role === 'row' && queryByRole(elem as HTMLElement, 'cell', { name: /default/i })
            )
            const contextName = getByRole(defaultRow, 'link', { name: 'global' })
            expect(contextName).toBeInTheDocument()
        })
    })
})
