import { DecoratorFn, Meta, Story } from '@storybook/react'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { H1, H2, H3, Text } from '..'
import { Alert } from '../Alert'
import { Button } from '../Button'

import { Container } from './Container'

const decorator: DecoratorFn = story => (
    <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
)

const config: Meta = {
    title: 'wildcard/Container',
    component: Container,
    decorators: [decorator],
}

export default config

export const Overview: Story = () => (
    <>
        <Alert variant="info">
            <Text>
                A container is meant to group content semantically together. Every page using it should have a header,
                optionally a description for the page and the container itself. Depending on the scope of a button, it
                should live inside or outside of the container.
            </Text>
            <Text>If the button</Text>
            <ul className="mb-0">
                <li>
                    affects everything inside the container (ie. saves all form fields within the container), it should
                    live outside of the container. See example 1
                </li>
                <li>
                    affects just a subset of content inside the container (ie. submits one of multiple forms), it should
                    live inside of the container, next to the content it is modifying. See example 2
                </li>
            </ul>
        </Alert>
        <hr />
        <H1>Example 1</H1>
        <H2>Some page explanation</H2>
        <Text className="text-muted">Optional: Add some descriptive text about what this page does.</Text>
        <Container className="mb-3">
            <H3>Section I</H3>
            <Text>Here you change the username.</Text>
            <div className="form-group">
                <input type="text" className="form-control" />
            </div>
            <H3>Section II</H3>
            <Text>Here you change your email.</Text>
            <div className="form-group mb-0">
                <input type="text" className="form-control" />
            </div>
        </Container>
        <div className="mb-3">
            <Button variant="primary" className="mr-2">
                Save
            </Button>
            <Button variant="secondary">Cancel</Button>
        </div>
        <hr />
        <H1>Example 2</H1>
        <H2>Some page explanation</H2>
        <Text className="text-muted">Optional: Add some descriptive text about what this page does.</Text>
        <Container className="mb-3">
            <H3>Section I</H3>
            <Text>Here you change the username.</Text>
            <div className="form-group">
                <input type="text" className="form-control" />
            </div>
            <Button className="mb-2" variant="secondary">
                Save
            </Button>
            <hr className="mb-2" />
            <H3>Section II</H3>
            <Text>Here you change your email.</Text>
            <div className="form-group">
                <input type="text" className="form-control" />
            </div>
            <Button variant="secondary">Save</Button>
        </Container>
    </>
)

Overview.parameters = {
    chromatic: {
        enableDarkMode: true,
        disableSnapshot: false,
    },
    design: {
        type: 'figma',
        name: 'Figma',
        url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/?node-id=1478%3A3044',
    },
}
