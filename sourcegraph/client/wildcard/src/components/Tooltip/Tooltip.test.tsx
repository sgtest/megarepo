import { render, RenderResult, cleanup, waitFor, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { wait } from '@testing-library/user-event/dist/utils'

import { Tooltip } from './Tooltip'

const TooltipTest = () => (
    <>
        Hover on{' '}
        <Tooltip content="Tooltip 1">
            <strong data-testid="trigger-1">me</strong>
        </Tooltip>
        , or{' '}
        <Tooltip content="Tooltip 2">
            <strong data-testid="trigger-2">me</strong>
        </Tooltip>
        , but nothing for{' '}
        <Tooltip content="">
            <strong data-testid="trigger-3">empty string</strong>
        </Tooltip>{' '}
        or{' '}
        <Tooltip content={null}>
            <strong data-testid="trigger-4">null</strong>
        </Tooltip>
    </>
)

describe('Tooltip', () => {
    let rendered: RenderResult

    afterEach(cleanup)

    beforeEach(() => {
        rendered = render(<TooltipTest />)
    })

    it('displays content when the trigger is hovered', async () => {
        userEvent.hover(rendered.getByTestId('trigger-1'))

        await waitFor(() => {
            expect(rendered.getByTestId('trigger-1')).toHaveAttribute('aria-describedby', 'radix-0')
            expect(rendered.getByTestId('trigger-2')).not.toHaveAttribute('aria-describedby')

            // Should be one tooltip for visual users, and a second for use with aria-describedby
            const tooltips = rendered.getAllByRole('tooltip')
            expect(tooltips).toHaveLength(2)
            expect(tooltips[0]).toHaveTextContent('Tooltip 1')
            expect(tooltips[1]).toHaveTextContent('Tooltip 1')
            expect(tooltips[1]).toHaveAttribute('id', 'radix-0')
        })

        userEvent.hover(rendered.getByTestId('trigger-2'))

        await waitFor(() => {
            expect(rendered.getByTestId('trigger-1')).not.toHaveAttribute('aria-describedby')
            expect(rendered.getByTestId('trigger-2')).toHaveAttribute('aria-describedby', 'radix-1')

            // Should be one tooltip for visual users, and a second for use with aria-describedby
            const tooltips = rendered.getAllByRole('tooltip')
            expect(tooltips).toHaveLength(2)
            expect(tooltips[0]).toHaveTextContent('Tooltip 2')
            expect(tooltips[1]).toHaveTextContent('Tooltip 2')
            expect(tooltips[1]).toHaveAttribute('id', 'radix-1')
        })
    })

    it('does not display a tooltip on hover for empty content', async () => {
        userEvent.hover(rendered.getByTestId('trigger-3'))
        await act(async () => {
            await wait(100)
        })
        expect(rendered.queryByRole('tooltip')).not.toBeInTheDocument()

        userEvent.hover(rendered.getByTestId('trigger-4'))
        await act(async () => {
            await wait(100)
        })
        expect(rendered.queryByRole('tooltip')).not.toBeInTheDocument()
    })

    it('hides content when the ESC key is pressed', async () => {
        userEvent.hover(rendered.getByTestId('trigger-1'))

        await waitFor(() => {
            expect(rendered.getAllByRole('tooltip')).toHaveLength(2)
        })

        userEvent.type(rendered.getByTestId('trigger-1'), '{esc}')

        await waitFor(() => {
            expect(rendered.queryByRole('tooltip')).not.toBeInTheDocument()
        })
    })

    it('does not hide content when the trigger is clicked', async () => {
        userEvent.hover(rendered.getByTestId('trigger-1'))

        await waitFor(() => {
            expect(rendered.getAllByRole('tooltip')).toHaveLength(2)
        })

        userEvent.click(rendered.getByTestId('trigger-1'))

        await waitFor(() => {
            expect(rendered.getAllByRole('tooltip')).toHaveLength(2)
        })
    })
})
