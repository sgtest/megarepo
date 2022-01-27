import { screen, render } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import React from 'react'
import sinon from 'sinon'

import { renderWithRouter } from '@sourcegraph/shared/src/testing/render-with-router'

import { ButtonLink } from './ButtonLink'

describe('<ButtonLink />', () => {
    test('renders correctly btn classes', () => {
        const { asFragment } = render(
            <ButtonLink to="http://example.com" variant="secondary" size="lg">
                Button link
            </ButtonLink>
        )
        expect(asFragment()).toMatchSnapshot()
    })
    test('renders correctly `disabled`', () => {
        const { asFragment } = render(
            <ButtonLink to="http://example.com" variant="secondary" size="lg" disabled={true}>
                Button link
            </ButtonLink>
        )
        expect(asFragment()).toMatchSnapshot()
    })
    test('renders correctly empty `to`', () => {
        const { asFragment } = render(
            <ButtonLink to={undefined} variant="secondary" size="lg">
                Button link
            </ButtonLink>
        )
        expect(asFragment()).toMatchSnapshot()
    })
    test('renders correctly anchor attributes', () => {
        const { asFragment } = renderWithRouter(
            <ButtonLink
                to="https://sourcegraph.com"
                variant="secondary"
                size="lg"
                target="_blank"
                rel="noopener noreferrer"
                data-tooltip="SourceGraph.com"
                data-pressed="true"
            >
                Button link
            </ButtonLink>
        )
        expect(asFragment()).toMatchSnapshot()
    })

    test('Should trigger onSelect', () => {
        const onSelect = sinon.stub()

        renderWithRouter(
            <ButtonLink
                to=""
                variant="secondary"
                size="lg"
                target="_blank"
                rel="noopener noreferrer"
                data-tooltip="SourceGraph.com"
                data-pressed="true"
                onClick={onSelect}
                data-testid="button-link"
            >
                Button link
            </ButtonLink>
        )

        userEvent.click(screen.getByTestId('button-link'))

        sinon.assert.calledOnce(onSelect)
    })
})
