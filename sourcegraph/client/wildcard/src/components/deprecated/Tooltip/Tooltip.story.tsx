import React, { useCallback, useEffect, useState } from 'react'

import { DecoratorFn, Meta, Story } from '@storybook/react'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { Button, Grid, Text, H1, H2, Code } from '../..'

import { TooltipController } from './TooltipController'

// BrandedStory already renders `<Tooltip />` so in Stories we don't render `<Tooltip />`
const decorator: DecoratorFn = story => (
    <BrandedStory styles={webStyles}>{() => <div className="p-5">{story()}</div>}</BrandedStory>
)

const config: Meta = {
    title: 'wildcard/Tooltip/Deprecated',

    decorators: [decorator],

    parameters: {
        component: 'DeprecatedTooltip',
        design: [
            {
                type: 'figma',
                name: 'Figma Light',
                url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Wildcard-Design-System?node-id=3131%3A38534',
            },
            {
                type: 'figma',
                name: 'Figma Dark',
                url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Wildcard-Design-System?node-id=3131%3A38727',
            },
        ],
    },
}

export default config

export const Basic: Story = () => (
    <Text>
        You can{' '}
        <strong data-tooltip="Tooltip 1" data-placement="right">
            hover me
        </strong>{' '}
        or <strong data-tooltip="Tooltip 2">me</strong>.
    </Text>
)

Basic.parameters = {
    chromatic: {
        disable: true,
    },
}

export const Positions: Story = () => (
    <>
        <H1>Tooltip</H1>
        <H2>Positions</H2>

        <Grid columnCount={4}>
            <div>
                <Button variant="secondary" data-placement="top" data-tooltip="Tooltip on top">
                    Tooltip on top
                </Button>
            </div>
            <div>
                <Button variant="secondary" data-placement="bottom" data-tooltip="Tooltip on bottom">
                    Tooltip on bottom
                </Button>
            </div>
            <div>
                <Button variant="secondary" data-placement="left" data-tooltip="Tooltip on left">
                    Tooltip on left
                </Button>
            </div>
            <div>
                <Button variant="secondary" data-placement="right" data-tooltip="Tooltip on right">
                    Tooltip on right
                </Button>
            </div>
        </Grid>

        <H2>Max width</H2>
        <Grid columnCount={1}>
            <div>
                <Button
                    variant="secondary"
                    data-tooltip="Nulla porttitor accumsan tincidunt. Proin eget tortor risus. Quisque velit nisi, pretium ut lacinia in, elementum id enim. Donec rutrum congue leo eget malesuada."
                >
                    Tooltip with long text
                </Button>
            </div>
        </Grid>
    </>
)

Positions.parameters = {
    chromatic: {
        disable: true,
    },
}

/*
    If you take a look at the handleEvent function in useTooltipState, you can see that the listeners are being added to the 'document',
    which means any 'mouseover/click' event will cause the tooltip to disappear.
*/
export const Pinned: Story = () => {
    const clickElement = useCallback((element: HTMLElement | null) => {
        if (element) {
            // The tooltip takes some time to set-up.
            // hence we need to delay the click by some ms.
            setTimeout(() => {
                element.click()
            }, 10)
        }
    }, [])

    return (
        <>
            <span data-tooltip="My tooltip" ref={clickElement}>
                Example
            </span>
            <Text>
                <small>
                    (A pinned tooltip is shown when the target element is rendered, without any user interaction
                    needed.)
                </small>
            </Text>
        </>
    )
}

Pinned.parameters = {
    chromatic: {
        // Chromatic pauses CSS animations by default and resets them to their initial state
        pauseAnimationAtEnd: true,
        enableDarkMode: true,
        disableSnapshot: false,
    },
}

const ForceUpdateTooltip = () => {
    const [copied, setCopied] = useState<boolean>(false)

    useEffect(() => {
        TooltipController.forceUpdate()
    }, [copied])

    const onClick: React.MouseEventHandler<HTMLButtonElement> = event => {
        event.preventDefault()

        setCopied(true)

        setTimeout(() => {
            setCopied(false)
        }, 1500)
    }

    return (
        <>
            <H2>
                Force update tooltip with <Code>TooltipController.forceUpdate()</Code>
            </H2>
            <Text>
                <Button variant="primary" onClick={onClick} data-tooltip={copied ? 'Copied!' : 'Click to copy'}>
                    Button
                </Button>
            </Text>
        </>
    )
}

export const Controller: Story = () => <ForceUpdateTooltip />

Controller.parameters = {
    chromatic: {
        disable: true,
    },
}
