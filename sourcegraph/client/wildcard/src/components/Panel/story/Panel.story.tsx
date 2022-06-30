import React from 'react'

import { mdiClose } from '@mdi/js'
import { useState } from '@storybook/addons'
import { DecoratorFn, Meta, Story } from '@storybook/react'
import classNames from 'classnames'
import { upperFirst } from 'lodash'

import { BrandedStory } from '@sourcegraph/branded/src/components/BrandedStory'
import { panels } from '@sourcegraph/branded/src/components/panel/TabbedPanelContent.fixtures'
import { EmptyPanelView } from '@sourcegraph/branded/src/components/panel/views/EmptyPanelView'
import webStyles from '@sourcegraph/web/src/SourcegraphWebApp.scss'

import { H1, H2, Tooltip } from '../..'
import { Button } from '../../Button'
import { Grid } from '../../Grid'
import { Icon } from '../../Icon'
import { Tabs, Tab, TabList, TabPanel, TabPanels } from '../../Tabs'
import { PANEL_POSITIONS } from '../constants'
import { Panel } from '../Panel'

import styles from './Story.module.scss'

const decorator: DecoratorFn = story => <BrandedStory styles={webStyles}>{() => <div>{story()}</div>}</BrandedStory>

const config: Meta = {
    title: 'wildcard/Panel',
    component: Panel,

    decorators: [decorator],

    parameters: {
        component: Panel,
        chromatic: {
            enableDarkMode: true,
            disableSnapshot: false,
        },
        design: [
            {
                type: 'figma',
                name: 'Figma Light',
                url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Wildcard-Design-System?node-id=3008%3A501',
            },
            {
                type: 'figma',
                name: 'Figma Dark',
                url: 'https://www.figma.com/file/NIsN34NH7lPu04olBzddTw/Wildcard-Design-System?node-id=3008%3A3223',
            },
        ],
    },
}

export default config

const PanelBodyContent: React.FunctionComponent<
    React.PropsWithChildren<{ position: typeof PANEL_POSITIONS[number] }>
> = ({ position, children }) => (
    <div
        className={classNames(
            'p-2',
            styles.panelBody,
            styles[`panelBody${upperFirst(position)}` as keyof typeof styles]
        )}
    >
        {children}
    </div>
)

export const Simple: Story = () => {
    const [position, setPosition] = useState<typeof PANEL_POSITIONS[number]>('left')

    const showPanelWithPosition = (postiion: typeof PANEL_POSITIONS[number]) => {
        setPosition(postiion)
    }

    return (
        <>
            <Grid columnCount={4}>
                <div />
                <div>
                    <H1>Panel</H1>
                    <H2>Positions</H2>
                    <div className="mb-2">
                        <Button variant="secondary" onClick={() => showPanelWithPosition('left')}>
                            Show left panel
                        </Button>
                    </div>
                    <div className="mb-2">
                        <Button variant="secondary" onClick={() => showPanelWithPosition('right')}>
                            Show right panel
                        </Button>
                    </div>
                    <div className="mb-2">
                        <Button variant="secondary" onClick={() => showPanelWithPosition('bottom')}>
                            Show bottom panel
                        </Button>
                    </div>
                </div>
                <div />
                <div />
            </Grid>
            <Panel
                isFloating={true}
                position={position}
                defaultSize={200}
                storageKey={`size-cache-${position}`}
                className={styles.panel}
            >
                <PanelBodyContent position={position}>
                    <b>{position}</b> panel content
                </PanelBodyContent>
            </Panel>
        </>
    )
}

export const WithChildren: Story = props => {
    const [tabIndex, setTabIndex] = React.useState(0)
    const activeTab = panels[tabIndex]

    const closePanel = () => setTabIndex(-1)

    return (
        <Panel {...props}>
            <Tabs index={tabIndex} size="small">
                <div className={classNames('tablist-wrapper d-flex justify-content-between sticky-top')}>
                    <TabList>
                        {panels.map((item, index) => (
                            <Tab key={item.id}>
                                <span
                                    className="tablist-wrapper--tab-label"
                                    onClick={() => setTabIndex(index)}
                                    role="none"
                                >
                                    {item.title}
                                </span>
                            </Tab>
                        ))}
                    </TabList>
                    <div className="align-items-center d-flex mr-2">
                        <Tooltip content="Close panel" placement="left">
                            <Button
                                onClick={closePanel}
                                className={classNames('ml-2')}
                                title="Close panel"
                                variant="icon"
                            >
                                <Icon aria-hidden={true} svgPath={mdiClose} />
                            </Button>
                        </Tooltip>
                    </div>
                </div>
                <TabPanels>
                    {activeTab ? (
                        panels.map(({ id, content }) => (
                            <TabPanel key={id}>
                                <Grid columnCount={3} spacing={2}>
                                    {new Array(6).fill(0).map((_value, index) => (
                                        <div key={index}>
                                            Content {index + 1} of {content}
                                        </div>
                                    ))}
                                </Grid>
                            </TabPanel>
                        ))
                    ) : (
                        <EmptyPanelView />
                    )}
                </TabPanels>
            </Tabs>
        </Panel>
    )
}
