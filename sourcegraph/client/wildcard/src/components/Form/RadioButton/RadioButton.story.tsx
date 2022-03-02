import { Meta, Story } from '@storybook/react'
import React, { useCallback } from 'react'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { Grid } from '../../Grid'

import { RadioButton, RadioButtonProps } from './RadioButton'

const config: Meta = {
    title: 'wildcard/RadioButton',

    decorators: [
        story => (
            <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
        ),
    ],

    parameters: {
        component: RadioButton,
        design: {
            type: 'figma',
            name: 'Figma',
            url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Wildcard-Design-System?node-id=854%3A2792',
        },
    },
}

export default config

const BaseRadio = ({ name, ...props }: Pick<RadioButtonProps, 'name' | 'isValid' | 'disabled'>) => {
    const [selected, setSelected] = React.useState('')

    const handleChange = useCallback<React.ChangeEventHandler<HTMLInputElement>>(event => {
        setSelected(event.target.value)
    }, [])

    return (
        <>
            <RadioButton
                id={`${name}-1`}
                name={name}
                value="first"
                checked={selected === 'first'}
                onChange={handleChange}
                label="First"
                message="Hello world!"
                {...props}
            />
            <RadioButton
                id={`${name}-2`}
                name={name}
                value="second"
                checked={selected === 'second'}
                onChange={handleChange}
                label="Second"
                message="Hello world!"
                {...props}
            />
            <RadioButton
                id={`${name}-3`}
                name={name}
                value="third"
                checked={selected === 'third'}
                onChange={handleChange}
                label="Third"
                message="Hello world!"
                {...props}
            />
        </>
    )
}

export const RadioExamples: Story = () => (
    <>
        <h1>Radio</h1>
        <Grid columnCount={4}>
            <div>
                <h2>Standard</h2>
                <BaseRadio name="standard-example" />
            </div>
            <div>
                <h2>Valid</h2>
                <BaseRadio name="valid-example" isValid={true} />
            </div>
            <div>
                <h2>Invalid</h2>
                <BaseRadio name="invalid-example" isValid={false} />
            </div>
            <div>
                <h2>Disabled</h2>
                <BaseRadio name="disabled-example" disabled={true} />
            </div>
        </Grid>
    </>
)

RadioExamples.parameters = {
    chromatic: {
        enableDarkMode: true,
        disableSnapshot: false,
    },
}
