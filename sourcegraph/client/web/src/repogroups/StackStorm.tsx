import React from 'react'

import { SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'

import { RepogroupPage, RepogroupPageProps } from './RepogroupPage'
import { RepogroupMetadata } from './types'

export const stackStorm: RepogroupMetadata = {
    title: 'StackStorm',
    name: 'stackstorm',
    url: '/stackstorm',
    description: '',
    examples: [
        {
            title: 'Passive sensor examples',
            patternType: SearchPatternType.literal,
            query: 'from st2reactor.sensor.base import Sensor',
        },
        {
            title: 'Polling sensor examples',
            patternType: SearchPatternType.literal,
            query: 'from st2reactor.sensor.base import PollingSensor',
        },
        {
            title: 'Trigger examples in rules',
            patternType: SearchPatternType.literal,
            query: 'repo:Exchange trigger: file:.yaml$',
        },
        {
            title: 'Actions that use the Orquesta runner',
            patternType: SearchPatternType.regexp,
            query: 'repo:Exchange runner_type:\\s*"orquesta"',
        },
        {
            title: 'All instances where a trigger is injected with an explicit payload',
            patternType: SearchPatternType.structural,
            query: 'repo:Exchange sensor_service.dispatch(:[1], payload=:[2])',
        },
    ],
    homepageDescription: 'Search within the StackStorm and StackStorm Exchange community.',
    homepageIcon: 'https://avatars.githubusercontent.com/u/4969009?s=200&v=4',
}

export const StackStormRepogroupPage: React.FunctionComponent<
    Omit<RepogroupPageProps, 'repogroupMetadata'>
> = props => <RepogroupPage {...props} repogroupMetadata={stackStorm} />
