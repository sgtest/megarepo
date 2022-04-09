import React from 'react'

export interface SeriesLikeChart<Datum> {
    data: Datum[]
    series: Series<Datum>[]
    getXValue: (datum: Datum) => Date
    stacked?: boolean

    /**
     * Callback runs whenever a point-zone (zone around point) and point itself
     * on the chart is clicked.
     */
    onDatumClick?: (event: React.MouseEvent) => void
}

export interface CategoricalLikeChart<Datum> {
    data: Datum[]
    getDatumValue: (datum: Datum) => number
    getDatumName: (datum: Datum) => string
    getDatumColor: (datum: Datum) => string | undefined
    getDatumLink?: (datum: Datum) => string | undefined | void
    onDatumLinkClick?: (event: React.MouseEvent) => void
}

export interface Series<Datum> {
    /**
     * The key in each data object for the values this line should be
     * calculated from.
     */
    dataKey: keyof Datum

    /**
     * The name of the line shown in the legend and tooltip
     */
    name: string

    /**
     * Link for data series point. It may be used to make datum points with links
     * instead of plain visual svg elements.
     */
    getLinkURL?: (datum: Datum, index: number) => string | undefined

    /**
     * The CSS color of the series. If color wasn't provided the default (gray) color
     * will be used instead.
     */
    color?: string
}
