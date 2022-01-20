import { screen } from '@testing-library/react'
import { createMemoryHistory, createLocation } from 'history'
import React from 'react'
import { NEVER } from 'rxjs'

import { renderWithRouter } from '@sourcegraph/shared/src/testing/render-with-router'

import { AuthenticatedUser } from '../../../auth'

import { CodeMonitorForm, CodeMonitorFormProps } from './CodeMonitorForm'

const PROPS: CodeMonitorFormProps = {
    history: createMemoryHistory(),
    location: createLocation('/code-monitoring/new'),
    onSubmit: () => NEVER,
    submitButtonLabel: '',
    authenticatedUser: {
        id: 'userID',
        username: 'username',
        email: 'user@me.com',
        siteAdmin: true,
    } as AuthenticatedUser,
}

describe('CodeMonitorForm', () => {
    test('Uses trigger query when present', () => {
        renderWithRouter(<CodeMonitorForm {...PROPS} triggerQuery="foo" />)
        expect(screen.getByTestId('trigger-query-edit')).toHaveValue('foo')
    })
})
