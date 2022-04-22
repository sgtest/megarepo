import { useState } from 'react'

import { Meta, Story } from '@storybook/react'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { Grid } from '../../Grid/Grid'

import { MultiSelect, MultiSelectProps, MultiSelectState, MultiSelectOption } from '.'

const config: Meta = {
    title: 'wildcard/MultiSelect',

    decorators: [
        story => (
            <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
        ),
    ],
}

export default config

type OptionValue = 'chocolate' | 'strawberry' | 'vanilla' | 'green tea' | 'rocky road' | 'really long'

const OPTIONS: MultiSelectOption<OptionValue>[] = [
    { value: 'chocolate', label: 'Chocolate' },
    { value: 'strawberry', label: 'Strawberry' },
    { value: 'vanilla', label: 'Vanilla' },
    { value: 'green tea', label: 'Green Tea' },
    { value: 'rocky road', label: 'Rocky Road' },
    { value: 'really long', label: 'A really really really REALLY long ice cream flavor' },
]

const BaseSelect = (props: Partial<Pick<MultiSelectProps, 'isValid' | 'isDisabled'>>) => {
    const [selectedOptions, setSelectedOptions] = useState<MultiSelectState<OptionValue>>([])

    return (
        <MultiSelect
            options={OPTIONS}
            defaultValue={selectedOptions}
            onChange={setSelectedOptions}
            message="I am a message"
            label="Select your favorite ice cream flavors."
            aria-label="Select your favorite ice cream flavors."
            {...props}
        />
    )
}

const SelectWithValues = () => {
    const [selectedOptions, setSelectedOptions] = useState<MultiSelectState<OptionValue>>([OPTIONS[5], OPTIONS[1]])

    return (
        <MultiSelect
            options={OPTIONS}
            defaultValue={selectedOptions}
            onChange={setSelectedOptions}
            message="I am a message"
            label="Select your favorite ice cream flavors."
            aria-label="Select your favorite ice cream flavors."
        />
    )
}

export const MultiSelectExamples: Story = () => (
    <>
        <h1>Multi Select</h1>
        <Grid columnCount={4}>
            <div>
                <h2>Standard</h2>
                <BaseSelect />
            </div>
            <div>
                <h2>Valid</h2>
                <BaseSelect isValid={true} />
            </div>
            <div>
                <h2>Invalid</h2>
                <BaseSelect isValid={false} />
            </div>
            <div>
                <h2>Disabled</h2>
                <BaseSelect isDisabled={true} />
            </div>
        </Grid>

        <h2>Pre-selected values (300px wide container)</h2>
        <div style={{ width: '300px ' }}>
            <SelectWithValues />
        </div>
    </>
)

MultiSelectExamples.parameters = {
    chromatic: {
        enableDarkMode: true,
        disableSnapshot: false,
    },
}
