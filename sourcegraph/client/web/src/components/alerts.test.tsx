import renderer from 'react-test-renderer'
import { ErrorAlert } from './alerts'
import React from 'react'
import { createMemoryHistory } from 'history'

jest.mock('mdi-react/AlertCircleIcon', () => 'AlertCircleIcon')

describe('ErrorAlert', () => {
    it('should render an Error object as an alert', () => {
        expect(
            renderer
                .create(<ErrorAlert error={new Error('an error happened')} history={createMemoryHistory()} />)
                .toJSON()
        ).toMatchSnapshot()
    })

    it('should add a prefix if given', () => {
        expect(
            renderer
                .create(
                    <ErrorAlert
                        error={new Error('an error happened')}
                        prefix="An error happened"
                        history={createMemoryHistory()}
                    />
                )
                .toJSON()
        ).toMatchSnapshot()
    })

    it('should omit the icon if icon={false}', () => {
        expect(
            renderer
                .create(
                    <ErrorAlert error={new Error('an error happened')} icon={false} history={createMemoryHistory()} />
                )
                .toJSON()
        ).toMatchSnapshot()
    })

    it('should render a Go multierror nicely', () => {
        expect(
            renderer
                .create(
                    <ErrorAlert
                        error={
                            new Error(
                                '- Additional property asdasd is not allowed\n- projectQuery.0: String length must be greater than or equal to 1\n'
                            )
                        }
                        history={createMemoryHistory()}
                    />
                )
                .toJSON()
        ).toMatchSnapshot()
    })
})
