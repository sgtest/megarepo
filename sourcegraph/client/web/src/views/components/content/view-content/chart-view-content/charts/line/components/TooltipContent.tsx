import { RenderTooltipParams } from '@visx/xychart/lib/components/Tooltip'
import classNames from 'classnames'
import React, { ReactElement } from 'react'

import { isDefined } from '@sourcegraph/shared/src/util/types'

import { DEFAULT_LINE_STROKE } from '../constants'
import { LineChartSeriesWithData, Point } from '../types'

import styles from './TooltipContent.module.scss'

export interface TooltipContentProps<Datum extends object> extends RenderTooltipParams<Point> {
    /** Dataset of series (lines) on the chart. */
    series: LineChartSeriesWithData<Datum>[]
    /** Possible className for root content element. */
    className?: string
}

/**
 * Display tooltip content for XYChart.
 * It consists of title - datetime for current x point and list of all nearest y points.
 */
export function TooltipContent<Datum extends object>(props: TooltipContentProps<Datum>): ReactElement | null {
    const { className = '', tooltipData, series } = props
    const datum = tooltipData?.nearestDatum?.datum

    if (!datum) {
        return null
    }

    const dateString = new Date(datum.x).toDateString()
    const lines = series
        .map(line => {
            const point = line.data.find(point => +point.x === +datum.x)

            if (!point) {
                return
            }

            return { ...line, point }
        })
        .filter(isDefined)

    return (
        <div className={classNames(styles.tooltipContent, className)}>
            <h3>{dateString}</h3>

            {/** values */}
            <ul className={styles.tooltipList}>
                {lines.map(line => {
                    const value = line.point.y
                    const datumKey = tooltipData?.nearestDatum?.key

                    /* eslint-disable react/forbid-dom-props */
                    return (
                        <li key={line.dataKey as string}>
                            <em
                                style={{
                                    color: line?.stroke ?? DEFAULT_LINE_STROKE,
                                    textDecoration: datumKey === line.dataKey ? 'underline' : undefined,
                                }}
                            >
                                {line?.name ?? 'unknown series'}
                            </em>{' '}
                            <span> {value === null || Number.isNaN(value) ? '–' : value} </span>
                        </li>
                    )
                })}
            </ul>
        </div>
    )
}
