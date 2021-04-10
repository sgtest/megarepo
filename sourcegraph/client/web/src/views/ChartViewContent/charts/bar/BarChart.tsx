import { AxisBottom, AxisLeft } from '@visx/axis'
import { localPoint } from '@visx/event'
import { GridRows } from '@visx/grid'
import { Group } from '@visx/group'
import { scaleBand, scaleLinear } from '@visx/scale'
import { Bar } from '@visx/shape'
import { useTooltip, TooltipWithBounds } from '@visx/tooltip'
import classnames from 'classnames'
import { range } from 'lodash'
import React, { ReactElement, useCallback, useMemo } from 'react'
import { BarChartContent } from 'sourcegraph'

import { onDatumClick } from '../types'

const DEFAULT_PADDING = { top: 20, right: 20, bottom: 25, left: 40 }

// Tooltip timeout used below as semaphore to prevent tooltip flashing
// in case if user is moving mouse very fast between bars
let tooltipTimeout: number

/** Data which needs to display tooltip with content. */
interface TooltipData {
    /** Label for current hovered bar */
    xLabel: string
    /** Y value for hovered bar */
    value: number
}

interface BarChartProps<Datum extends object> extends Omit<BarChartContent<Datum, keyof Datum>, 'chart'> {
    /** Chart width in px. */
    width: number
    /** Chart height in px. */
    height: number
    /** Callback calls every time when a bar on the chart was clicked */
    onDatumClick: onDatumClick
}

/**
 * Displays bar chart with tooltip.
 */
export function BarChart<Datum extends object>(props: BarChartProps<Datum>): ReactElement {
    const {
        width,
        height,
        data,
        series,
        onDatumClick,
        xAxis: { dataKey: xDataKey },
    } = props

    // Respect only first element of data series
    // Refactor this in case if we need support stacked bar chart
    const { dataKey, fill, linkURLs } = series[0]

    const innerWidth = width - DEFAULT_PADDING.left - DEFAULT_PADDING.right
    const innerHeight = height - DEFAULT_PADDING.top - DEFAULT_PADDING.bottom

    const { tooltipOpen, tooltipLeft, tooltipTop, tooltipData, hideTooltip, showTooltip } = useTooltip<TooltipData>()

    // Get access to y value of each bar (datum)
    const yAccessor = useCallback((data: Datum): number => +data[dataKey], [dataKey])
    const formatXLabel = useCallback((index: number): string => (data[index][xDataKey] as unknown) as string, [
        data,
        xDataKey,
    ])

    // Create x (band) d3 scale (see https://observablehq.com/@d3/d3-scaleband)
    // used below to place x axis label and bars in right position on the chart
    const xScale = useMemo(
        () =>
            scaleBand({
                range: [0, innerWidth],
                round: true,
                domain: range(data.length),
                padding: 0.2,
            }),
        [innerWidth, data]
    )

    // Create y linear d3 scale (see https://observablehq.com/@d3/d3-scalelinear)
    // used below to calculate bar height according data and inner height of the chart
    const yScale = useMemo(
        () =>
            scaleLinear({
                range: [innerHeight, 0],
                round: true,
                nice: true,
                domain: [0, Math.max(...data.map(yAccessor))],
            }),
        [innerHeight, data, yAccessor]
    )

    // handlers
    const handleMouseLeave = (): void => {
        tooltipTimeout = window.setTimeout(() => {
            hideTooltip()
        }, 300)
    }

    return (
        <div className="bar-chart">
            <svg width={width} height={height}>
                <Group left={DEFAULT_PADDING.left} top={DEFAULT_PADDING.top}>
                    <GridRows scale={yScale} width={innerWidth} height={innerHeight} className="bar-chart__grid" />

                    {data.map((datum, index) => {
                        const barHeight = innerHeight - (yScale(yAccessor(datum)) ?? 0)
                        const link = linkURLs?.[index]
                        const classes = classnames('bar-chart__bar', { 'bar-chart__bar--with-link': link })

                        return (
                            <Group key={`bar-${index}`}>
                                <Bar
                                    className={classes}
                                    x={xScale(index)}
                                    y={innerHeight - barHeight}
                                    height={barHeight}
                                    width={xScale.bandwidth()}
                                    fill={fill}
                                    /* eslint-disable-next-line react/jsx-no-bind */
                                    onClick={event => {
                                        const link = linkURLs?.[index]

                                        onDatumClick({ originEvent: event, link })
                                    }}
                                    /* eslint-disable-next-line react/jsx-no-bind */
                                    onMouseLeave={handleMouseLeave}
                                    /* eslint-disable-next-line react/jsx-no-bind */
                                    onMouseMove={event => {
                                        if (tooltipTimeout) {
                                            clearTimeout(tooltipTimeout)
                                        }

                                        const rectangle = localPoint(event)

                                        showTooltip({
                                            tooltipData: { xLabel: formatXLabel(index), value: yAccessor(datum) },
                                            tooltipTop: rectangle?.y,
                                            tooltipLeft: rectangle?.x,
                                        })
                                    }}
                                />
                            </Group>
                        )
                    })}

                    <AxisBottom
                        top={innerHeight}
                        scale={xScale}
                        tickFormat={formatXLabel}
                        axisClassName="bar-chart__axis"
                        axisLineClassName="bar-chart__axis-line"
                        tickClassName="bar-chart__axis-tick"
                    />

                    <AxisLeft
                        scale={yScale}
                        axisClassName="bar-chart__axis"
                        axisLineClassName="bar-chart__axis-line"
                        tickClassName="bar-chart__axis-tick"
                    />
                </Group>
            </svg>

            {tooltipOpen && tooltipData && (
                <TooltipWithBounds className="bar-chart__tooltip" top={tooltipTop} left={tooltipLeft}>
                    <div className="bar-chart__tooltip-content">
                        <strong className="bar-chart__tooltip-name">{tooltipData.xLabel}</strong>
                    </div>

                    <div className="bar-chart__tooltip-value">{tooltipData.value}</div>
                </TooltipWithBounds>
            )}
        </div>
    )
}
