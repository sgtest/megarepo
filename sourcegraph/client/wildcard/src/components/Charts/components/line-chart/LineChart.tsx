import { ReactElement, useMemo, SVGProps, CSSProperties, FocusEvent, useCallback, useState } from 'react'

import { scaleTime, scaleLinear, getTicks } from '@visx/scale'
import { AnyD3Scale } from '@visx/scale/lib/types/Scale'
import classNames from 'classnames'
import { ScaleLinear, ScaleTime } from 'd3-scale'
import { timeFormat } from 'd3-time-format'
import { noop } from 'lodash'

import { Padding } from '../../../Popover'
import { Tooltip } from '../../../Tooltip'
import { SvgAxisBottom, SvgAxisLeft, SvgContent, SvgRoot } from '../../core'
import { Series, SeriesLikeChart } from '../../types'

import { getSortedByFirstPointSeries } from './keyboard-navigation'
import { LineChartContent } from './LineChartContent'
import { Point } from './types'
import { getSeriesData, getMinMaxBoundaries } from './utils'

import styles from './LineChart.module.scss'

/**
 * Returns a formatted time text. It's used primary for X axis tick's text nodes.
 * Number of month day + short name of month.
 *
 * Example: 01 Jan, 12 Feb, ...
 */
const formatDateTick = timeFormat('%d %b')

interface GetScaleTicksInput {
    scale: AnyD3Scale
    space: number
    pixelsPerTick?: number
}

export function getXScaleTicks<T>(input: GetScaleTicksInput): T[] {
    const { scale, space, pixelsPerTick = 80 } = input
    // Calculate desirable number of ticks
    const numberTicks = Math.max(2, Math.floor(space / pixelsPerTick))
    return getTicks(scale, numberTicks) as T[]
}

interface GetLineGroupStyleOptions {
    /** Whether this series contains the active point */
    id: string

    /** The id of the series */
    hasActivePoint: boolean

    /** Whether the chart has some active point */
    isActive: boolean
}

export interface LineChartProps<Datum> extends SeriesLikeChart<Datum>, SVGProps<SVGSVGElement> {
    /** The width of the chart */
    width: number

    /** The height of the chart */
    height: number

    /** Whether to start Y axis at zero */
    zeroYAxisMin?: boolean

    activeSeriesId?: string

    /** Function to style a given line group */
    getLineGroupStyle?: (options: GetLineGroupStyleOptions) => CSSProperties

    /**
     * If provided, uses this to render lines on the chart instead of `series`
     *
     * @param dataSeries a SeriesWithData array containing the data to render
     * @returns a SeriesWithData array that has been filtered
     */
    getActiveSeries?: <S extends Pick<Series<Datum>, 'id'>>(dataSeries: S[]) => S[]

    /** Visual content padding for the SVG element */
    padding?: Padding
}

const identity = <T,>(argument: T): T => argument
const DEFAULT_LINE_CHART_PADDING = { top: 16, right: 18, bottom: 0, left: 0 }

/**
 * Visual component that renders svg line chart with pre-defined sizes, tooltip,
 * voronoi area distribution.
 */
export function LineChart<D>(props: LineChartProps<D>): ReactElement | null {
    const {
        width,
        height,
        series,
        activeSeriesId,
        stacked = false,
        zeroYAxisMin = false,
        getLineGroupStyle,
        getActiveSeries = identity,
        padding = DEFAULT_LINE_CHART_PADDING,
        onDatumClick = noop,
        className,
        ...attributes
    } = props

    const [isTooltipOpen, setTooltipOpen] = useState(false)
    const dataSeries = useMemo(
        // Sort series by their first element value in order to render series
        // with the lowest point first, to adjust native browser focus order
        () => getSortedByFirstPointSeries(getSeriesData({ series, stacked })),
        [series, stacked]
    )

    const { minX, maxX, minY, maxY } = useMemo(
        () => getMinMaxBoundaries({ dataSeries, zeroYAxisMin }),
        [dataSeries, zeroYAxisMin]
    )

    const xScale = useMemo(
        () =>
            scaleTime<number>({
                domain: [minX, maxX],
                nice: true,
                clamp: true,
            }),
        [minX, maxX]
    )

    const yScale = useMemo(
        () =>
            scaleLinear<number>({
                domain: [minY, maxY],
                nice: true,
                clamp: true,
            }),
        [minY, maxY]
    )

    const handleDatumAreaClick = useCallback(
        (point?: Point) => {
            // Since click has been fired not by native link but by a click in the
            // link area, we should synthetically trigger the link behavior
            if (point?.linkUrl) {
                window.open(point.linkUrl)
            }

            onDatumClick(point)
        },
        [onDatumClick]
    )

    const handleSvgFocus = (event: FocusEvent): void => {
        if (event.currentTarget === event.target) {
            setTooltipOpen(true)
        }
    }

    return (
        <Tooltip open={isTooltipOpen} content="Use arrow keys to navigate through the Y/X axes.">
            <SvgRoot
                {...attributes}
                role="group"
                width={width}
                height={height}
                xScale={xScale}
                yScale={yScale}
                padding={padding}
                className={classNames(styles.root, className)}
                onFocus={handleSvgFocus}
                onBlur={() => setTooltipOpen(false)}
            >
                <SvgAxisLeft />

                <SvgAxisBottom
                    pixelsPerTick={70}
                    minRotateAngle={20}
                    maxRotateAngle={60}
                    tickFormat={formatDateTick}
                    getScaleTicks={getXScaleTicks}
                />

                <SvgContent>
                    {({ xScale, yScale, content }) => (
                        <LineChartContent<D>
                            width={content.width}
                            height={content.height}
                            top={content.top}
                            left={content.left}
                            stacked={stacked}
                            xScale={xScale as ScaleTime<number, number>}
                            yScale={yScale as ScaleLinear<number, number>}
                            dataSeries={dataSeries}
                            activeSeriesId={activeSeriesId}
                            getActiveSeries={getActiveSeries}
                            getLineGroupStyle={getLineGroupStyle}
                            onDatumClick={onDatumClick}
                            onDatumAreaClick={handleDatumAreaClick}
                        />
                    )}
                </SvgContent>
            </SvgRoot>
        </Tooltip>
    )
}
