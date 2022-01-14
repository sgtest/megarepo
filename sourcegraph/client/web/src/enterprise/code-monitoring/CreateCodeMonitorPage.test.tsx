import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import * as H from 'history'
import * as React from 'react'
import { act } from 'react-dom/test-utils'
import { NEVER, of } from 'rxjs'
import sinon from 'sinon'

import { AuthenticatedUser } from '../../auth'
import { CreateCodeMonitorVariables } from '../../graphql-operations'

import { CreateCodeMonitorPage } from './CreateCodeMonitorPage'
import { mockCodeMonitor } from './testing/util'

describe('CreateCodeMonitorPage', () => {
    const mockUser = {
        id: 'userID',
        username: 'username',
        email: 'user@me.com',
        siteAdmin: true,
    } as AuthenticatedUser

    const history = H.createMemoryHistory()
    const props = {
        location: history.location,
        authenticatedUser: mockUser,
        breadcrumbs: [{ depth: 0, breadcrumb: null }],
        setBreadcrumb: sinon.spy(),
        useBreadcrumb: sinon.spy(),
        history,
        deleteCodeMonitor: sinon.spy((id: string) => NEVER),
        createCodeMonitor: sinon.spy((monitor: CreateCodeMonitorVariables) =>
            of({ description: mockCodeMonitor.node.description })
        ),
    }
    let clock: sinon.SinonFakeTimers

    beforeEach(() => {
        clock = sinon.useFakeTimers()
    })

    afterEach(() => {
        clock.restore()
    })

    afterEach(() => {
        props.createCodeMonitor.resetHistory()
    })

    test('createCodeMonitor is called on submit', () => {
        render(<CreateCodeMonitorPage {...props} />)
        const nameInput = screen.getByTestId('name-input')
        userEvent.type(nameInput, 'Test updated')
        userEvent.click(screen.getByTestId('trigger-button'))

        const triggerInput = screen.getByTestId('trigger-query-edit')
        expect(triggerInput).toBeInTheDocument()

        userEvent.type(triggerInput, 'test type:diff repo:test')
        act(() => {
            clock.tick(600)
        })

        expect(triggerInput).toHaveClass('test-is-valid')

        userEvent.click(screen.getByTestId('submit-trigger'))

        userEvent.click(screen.getByTestId('form-action-toggle-email-notification'))

        userEvent.click(screen.getByTestId('submit-action'))

        act(() => {
            clock.tick(600)
        })

        userEvent.click(screen.getByTestId('submit-monitor'))

        sinon.assert.called(props.createCodeMonitor)
    })

    test('createCodeMonitor is not called on submit when trigger or action is incomplete', () => {
        render(<CreateCodeMonitorPage {...props} />)
        const nameInput = screen.getByTestId('name-input')
        userEvent.type(nameInput, 'Test updated')
        userEvent.click(screen.getByTestId('submit-monitor'))

        // Pressing enter does not call createCodeMonitor because other fields not complete
        sinon.assert.notCalled(props.createCodeMonitor)

        userEvent.click(screen.getByTestId('trigger-button'))

        const triggerInput = screen.getByTestId('trigger-query-edit')
        expect(triggerInput).toBeInTheDocument()

        userEvent.type(triggerInput, 'test type:diff repo:test')
        act(() => {
            clock.tick(600)
        })
        expect(triggerInput).toHaveClass('test-is-valid')
        userEvent.click(screen.getByTestId('submit-trigger'))

        userEvent.click(screen.getByTestId('submit-monitor'))

        // Pressing enter still does not call createCodeMonitor
        sinon.assert.notCalled(props.createCodeMonitor)

        userEvent.click(screen.getByTestId('form-action-toggle-email-notification'))
        userEvent.click(screen.getByTestId('submit-action'))

        act(() => {
            clock.tick(600)
        })

        // Pressing enter calls createCodeMonitor when all sections are complete
        userEvent.click(screen.getByTestId('submit-monitor'))

        sinon.assert.calledOnce(props.createCodeMonitor)
    })

    test('Actions area button is disabled while trigger is incomplete', () => {
        render(<CreateCodeMonitorPage {...props} />)
        const actionButton = screen.getByTestId('form-action-toggle-email-notification')
        expect(actionButton).toHaveClass('disabled')
    })
})
