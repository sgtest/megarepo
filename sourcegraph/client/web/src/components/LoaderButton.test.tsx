import { renderWithBrandedContext } from '@sourcegraph/shared/src/testing'

import { LoaderButton } from './LoaderButton'

describe('LoaderButton', () => {
    it('should render a loading spinner when loading prop is true', () => {
        expect(renderWithBrandedContext(<LoaderButton label="Test" loading={true} />).asFragment()).toMatchSnapshot()
    })

    it('should not render a loading spinner when loading prop is false', () => {
        expect(renderWithBrandedContext(<LoaderButton label="Test" loading={false} />).asFragment()).toMatchSnapshot()
    })
})
