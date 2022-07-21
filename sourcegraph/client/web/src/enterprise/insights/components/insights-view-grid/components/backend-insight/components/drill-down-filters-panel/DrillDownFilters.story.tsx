import { useRef } from 'react'

import { Meta, Story } from '@storybook/react'

import { WebStory } from '../../../../../../../../components/WebStory'
import { SeriesSortDirection, SeriesSortMode } from '../../../../../../../../graphql-operations'
import { InsightFilters } from '../../../../../../core'
import { DrillDownFiltersPopover } from '../drill-down-filters-popover/DrillDownFiltersPopover'

const defaultStory: Meta = {
    title: 'web/insights/DrillDownInsightFilters',
    decorators: [story => <WebStory>{() => story()}</WebStory>],
}

export default defaultStory

export const DrillDownPopover: Story = () => {
    const exampleReference = useRef(null)
    const initialFiltersValue: InsightFilters = {
        excludeRepoRegexp: 'EXCLUDE',
        includeRepoRegexp: '',
        context: '',
        seriesDisplayOptions: {
            limit: '20',
            sortOptions: {
                direction: SeriesSortDirection.DESC,
                mode: SeriesSortMode.RESULT_COUNT,
            },
        },
    }

    return (
        <DrillDownFiltersPopover
            isOpen={true}
            anchor={exampleReference}
            initialFiltersValue={initialFiltersValue}
            originalFiltersValue={initialFiltersValue}
            onFilterChange={log('onFilterChange')}
            onFilterSave={log('onFilterSave')}
            onInsightCreate={log('onInsightCreate')}
            onVisibilityChange={log('onVisibilityChange')}
        />
    )
}

// eslint-disable-next-line arrow-body-style
const log = (methodName: string) => {
    return function (args: unknown) {
        // eslint-disable-next-line prefer-rest-params
        console.log(methodName, [...arguments])
    }
}
