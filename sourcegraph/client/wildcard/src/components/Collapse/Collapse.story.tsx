import { useCallback, useState } from 'react'

import { DecoratorFn, Meta, Story } from '@storybook/react'
import ChevronDownIcon from 'mdi-react/ChevronDownIcon'
import ChevronLeftIcon from 'mdi-react/ChevronLeftIcon'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { Button } from '../Button'
import { Input } from '../Form'
import { Icon } from '../Icon'

import { Collapse, CollapseHeader, CollapsePanel } from './Collapse'

const decorator: DecoratorFn = story => (
    <BrandedStory styles={webStyles}>{() => <div className="container mt-3">{story()}</div>}</BrandedStory>
)

const config: Meta = {
    title: 'wildcard/Collapse',
    component: Collapse,

    decorators: [decorator],
}

export default config

export const Simple: Story = () => {
    const [isOpened, setIsOpened] = useState(false)

    const handleOpenChange = useCallback((next: boolean) => {
        setIsOpened(next)
    }, [])

    return (
        <div>
            <h2 className="my-3">Controlled collapse</h2>
            <Collapse isOpen={isOpened} onOpenChange={handleOpenChange}>
                <CollapseHeader as={Button} outline={true} focusLocked={true} variant="secondary" className="w-50">
                    Collapsable
                    <Icon as={isOpened ? ChevronDownIcon : ChevronLeftIcon} className="mr-1" />
                </CollapseHeader>
                <CollapsePanel className="w-50">
                    <Input placeholder="testing this one" />
                </CollapsePanel>
            </Collapse>

            <h2 className="my-3">Uncontrolled collapse</h2>
            <Collapse>
                {({ isOpen }) => (
                    <>
                        <CollapseHeader
                            as={Button}
                            aria-label={isOpen ? 'Expand' : 'Collapse'}
                            outline={true}
                            variant="secondary"
                            className="w-50"
                        >
                            Collapsable
                            <Icon as={isOpen ? ChevronDownIcon : ChevronLeftIcon} className="mr-1" />
                        </CollapseHeader>
                        <CollapsePanel className="w-50">
                            <Input placeholder="testing this one" />
                        </CollapsePanel>
                    </>
                )}
            </Collapse>

            <h2 className="my-3">Open by default collapse</h2>
            <Collapse openByDefault={true}>
                {({ isOpen }) => (
                    <>
                        <CollapseHeader
                            as={Button}
                            aria-label={isOpen ? 'Expand' : 'Collapse'}
                            outline={true}
                            variant="secondary"
                            className="w-50"
                        >
                            Collapsable
                            <Icon as={isOpen ? ChevronDownIcon : ChevronLeftIcon} className="mr-1" />
                        </CollapseHeader>
                        <CollapsePanel className="w-50">
                            <Input placeholder="testing this one" />
                        </CollapsePanel>
                    </>
                )}
            </Collapse>
        </div>
    )
}
