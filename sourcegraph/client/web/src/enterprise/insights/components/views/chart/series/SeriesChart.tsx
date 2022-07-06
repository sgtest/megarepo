import React, { SVGProps, useCallback } from 'react'

import { LineChart, SeriesLikeChart } from '../../../../../../charts'
import { LineChartProps } from '../../../../../../charts/components/line-chart/LineChart'
import { SeriesWithData } from '../../../../../../charts/components/line-chart/utils'
import { UseSeriesToggleReturn } from '../../../../../../insights/utils/use-series-toggle'
import { LockedChart } from '../locked/LockedChart'

export enum SeriesBasedChartTypes {
    Line,
}

export interface SeriesChartProps<D> extends SeriesLikeChart<D>, Omit<SVGProps<SVGSVGElement>, 'type'> {
    type: SeriesBasedChartTypes
    width: number
    height: number
    zeroYAxisMin?: boolean
    locked?: boolean
    seriesToggleState?: UseSeriesToggleReturn
}

const FULL_COLOR = 1
const DIMMED_COLOR = 0.5
const DEFAULT_TRUE_GETTER = (): true => true

export function SeriesChart<Datum>(props: SeriesChartProps<Datum>): React.ReactElement {
    const { series, type, locked, seriesToggleState, ...otherProps } = props

    const { isSeriesHovered = DEFAULT_TRUE_GETTER, isSeriesSelected = DEFAULT_TRUE_GETTER, hoveredId } =
        seriesToggleState || {}

    const getOpacity = (id: string, hasActivePoint: boolean, isActive: boolean): number => {
        if (hoveredId && !isSeriesHovered(id)) {
            return DIMMED_COLOR
        }

        // Highlight series with active point
        if (hasActivePoint) {
            if (isActive) {
                return FULL_COLOR
            }

            return DIMMED_COLOR
        }

        if (isSeriesSelected(id)) {
            return FULL_COLOR
        }

        if (isSeriesHovered(id)) {
            return DIMMED_COLOR
        }

        return FULL_COLOR
    }

    const getHoverStyle: LineChartProps<Datum>['getLineGroupStyle'] = ({ id, hasActivePoint, isActive }) => {
        const opacity = getOpacity(id, hasActivePoint, isActive)

        return {
            opacity,
            transitionProperty: 'opacity',
            transitionDuration: '200ms',
            transitionTimingFunction: 'ease-out',
        }
    }

    const getActiveSeries = useCallback(
        <D,>(dataSeries: SeriesWithData<D>[]): SeriesWithData<D>[] =>
            dataSeries.filter(series => isSeriesSelected(`${series.id}`) || isSeriesHovered(`${series.id}`)),
        [isSeriesSelected, isSeriesHovered]
    )

    if (locked) {
        return <LockedChart />
    }

    return (
        <LineChart
            series={series}
            getLineGroupStyle={getHoverStyle}
            getActiveSeries={getActiveSeries}
            {...otherProps}
        />
    )
}
