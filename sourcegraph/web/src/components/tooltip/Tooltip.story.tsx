import { storiesOf } from '@storybook/react'
import React, { useCallback } from 'react'
import { Tooltip } from './Tooltip'

import './Tooltip.scss'

const { add } = storiesOf('Tooltip', module).addDecorator(story => (
    <div style={{ maxWidth: '20rem', margin: '2rem' }}>{story()}</div>
))

add('Hover', () => (
    <>
        <Tooltip />
        <p>
            You can <strong data-tooltip="Tooltip 1">hover me</strong> or <strong data-tooltip="Tooltip 2">me</strong>.
        </p>
    </>
))

const PinnedTooltip: React.FunctionComponent = () => {
    const clickElement = useCallback((e: HTMLElement | null) => {
        if (e) {
            e.click()
        }
    }, [])
    return (
        <>
            <Tooltip />
            <span data-tooltip="My tooltip" ref={clickElement}>
                Example
            </span>
            <p>
                <small>
                    (A pinned tooltip is shown when the target element is rendered, without any user interaction
                    needed.)
                </small>
            </p>
        </>
    )
}
add('Pinned', () => <PinnedTooltip />)
