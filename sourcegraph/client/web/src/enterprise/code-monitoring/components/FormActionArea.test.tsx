import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import sinon from 'sinon'

import { renderWithBrandedContext } from '@sourcegraph/shared/src/testing'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'

import { CodeMonitorFields } from '../../../graphql-operations'
import { mockAuthenticatedUser } from '../testing/util'

import { FormActionArea } from './FormActionArea'

describe('FormActionArea', () => {
    const mockActions: CodeMonitorFields['actions'] = {
        nodes: [
            {
                __typename: 'MonitorEmail',
                id: 'id1',
                recipients: { nodes: [{ id: mockAuthenticatedUser.id }] },
                enabled: true,
                includeResults: false,
            },
        ],
    }

    test('Error is shown if code monitor has empty description', () => {
        const { asFragment } = renderWithBrandedContext(
            <MockedTestProvider>
                <FormActionArea
                    actions={mockActions}
                    actionsCompleted={true}
                    setActionsCompleted={sinon.spy()}
                    disabled={false}
                    authenticatedUser={mockAuthenticatedUser}
                    onActionsChange={sinon.spy()}
                    monitorName=""
                />
            </MockedTestProvider>
        )

        userEvent.click(screen.getByTestId('form-action-toggle-email'))

        expect(asFragment()).toMatchSnapshot()
    })
})
