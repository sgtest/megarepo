import { Meta } from '@storybook/react'
import React, { useCallback } from 'react'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { Grid } from '../../Grid'

import { Checkbox, CheckboxProps } from './Checkbox'

const config: Meta = {
    title: 'wildcard/Checkbox',

    decorators: [
        story => (
            <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
        ),
    ],

    parameters: {
        component: Checkbox,
        chromatic: {
            enableDarkMode: true,
        },
        design: {
            type: 'figma',
            name: 'Figma',
            url:
                'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Design-Refresh-Systemization-source-of-truth?node-id=908%3A1353',
        },
    },
}

export default config

const BaseCheckbox = ({ name, ...props }: { name: string } & Pick<CheckboxProps, 'isValid' | 'disabled'>) => {
    const [isChecked, setChecked] = React.useState(false)

    const handleChange = useCallback<React.ChangeEventHandler<HTMLInputElement>>(event => {
        setChecked(event.target.checked)
    }, [])

    return (
        <Checkbox
            name={name}
            id={name}
            value="first"
            checked={isChecked}
            onChange={handleChange}
            label="Check me!"
            message="Hello world!"
            {...props}
        />
    )
}

export const CheckboxExamples: React.FunctionComponent = () => (
    <>
        <h1>Checkbox</h1>
        <Grid columnCount={4}>
            <div>
                <h2>Standard</h2>
                <BaseCheckbox name="standard-example" />
            </div>
            <div>
                <h2>Valid</h2>
                <BaseCheckbox name="valid-example" isValid={true} />
            </div>
            <div>
                <h2>Invalid</h2>
                <BaseCheckbox name="invalid-example" isValid={false} />
            </div>
            <div>
                <h2>Disabled</h2>
                <BaseCheckbox name="disabled-example" disabled={true} />
            </div>
        </Grid>
    </>
)
