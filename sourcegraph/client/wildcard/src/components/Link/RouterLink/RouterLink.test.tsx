import { render } from '@testing-library/react'
import React from 'react'

import { renderWithRouter } from '@sourcegraph/shared/src/testing/render-with-router'

import { RouterLink } from './RouterLink'

describe('RouterLink', () => {
    it('renders router link correctly', () => {
        const { asFragment } = renderWithRouter(<RouterLink to="/docs">Link to docs</RouterLink>)
        expect(asFragment()).toMatchSnapshot()
    })
    it('renders absolute URL correctly ', () => {
        const { asFragment } = render(<RouterLink to="https://sourcegraph.com">SourceGraph</RouterLink>)
        expect(asFragment()).toMatchSnapshot()
    })
})
