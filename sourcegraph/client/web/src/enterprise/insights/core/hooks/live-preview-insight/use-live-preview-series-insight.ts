import { useMemo } from 'react'

import { ApolloError, gql, useQuery } from '@apollo/client'
import { Duration } from 'date-fns'

import { HTTPStatusError } from '@sourcegraph/http-client'
import { Series } from '@sourcegraph/wildcard'

import {
    GetInsightPreviewResult,
    GetInsightPreviewVariables,
    SearchSeriesPreviewInput,
} from '../../../../../graphql-operations'
import { DATA_SERIES_COLORS_LIST, MAX_NUMBER_OF_SERIES } from '../../../constants'
import { getStepInterval } from '../../backend/gql-backend/utils/get-step-interval'
import { generateLinkURL, InsightDataSeriesData } from '../../backend/utils/create-line-chart-content'

import { LivePreviewStatus, State } from './types'

export const GET_INSIGHT_PREVIEW_GQL = gql`
    query GetInsightPreview($input: SearchInsightPreviewInput!) {
        searchInsightPreview(input: $input) {
            points {
                dateTime
                value
                diffQuery
            }
            label
        }
    }
`

export interface SeriesWithStroke extends SearchSeriesPreviewInput {
    stroke?: string
}

interface Props {
    skip: boolean
    step: Duration
    repositories: string[]
    series: SeriesWithStroke[]
}

interface Result<R> {
    state: State<R>
    refetch: () => {}
}

/**
 * Series insight (search based and capture group insights) live preview hook.
 * It's used primarily for presenting insight live preview data in the creation UI pages.
 *
 * All data for insight live preview isn't stored in the code insights DB tables
 * instead, it's calculated on the fly in query time on the backend.
 */
export function useLivePreviewSeriesInsight(props: Props): Result<Series<Datum>[]> {
    const { skip, repositories, step, series } = props
    const [unit, value] = getStepInterval(step)

    const { data, loading, error, refetch } = useQuery<GetInsightPreviewResult, GetInsightPreviewVariables>(
        GET_INSIGHT_PREVIEW_GQL,
        {
            skip,
            variables: {
                input: {
                    series: series.map(srs => ({
                        query: srs.query,
                        label: srs.label,
                        generatedFromCaptureGroups: srs.generatedFromCaptureGroups,
                        groupBy: srs.groupBy,
                    })),
                    repositoryScope: { repositories },
                    timeScope: { stepInterval: { unit, value: +value } },
                },
            },
        }
    )

    const parsedSeries = useMemo(() => {
        if (data) {
            return createPreviewSeriesContent({
                response: data,
                originalSeries: series,
                repositories,
            })
        }

        return null
    }, [data, repositories, series])

    if (loading) {
        return { state: { status: LivePreviewStatus.Loading }, refetch }
    }

    if (error) {
        if (isGatewayTimeoutError(error)) {
            return {
                state: {
                    status: LivePreviewStatus.Error,
                    error: new Error(
                        'Live preview is not available for this chart as it did not complete in the allowed time'
                    ),
                },
                refetch,
            }
        }

        return { state: { status: LivePreviewStatus.Error, error }, refetch }
    }

    if (parsedSeries) {
        return { state: { status: LivePreviewStatus.Data, data: parsedSeries }, refetch }
    }

    return { state: { status: LivePreviewStatus.Intact }, refetch }
}

export interface Datum {
    dateTime: Date
    value: number
    link?: string
}

interface PreviewProps {
    response: GetInsightPreviewResult
    originalSeries: SeriesWithStroke[]
    repositories: string[]
}

function createPreviewSeriesContent(props: PreviewProps): Series<Datum>[] {
    const { response, originalSeries } = props
    const { searchInsightPreview: series } = response

    // inputMetadata creates a lookup so that the correct color can be later applied to the preview series
    const inputMetadata = Object.fromEntries(
        originalSeries.map((previewSeries, index) => [`${previewSeries.label}-${index}`, previewSeries])
    )

    // Extend series with synthetic index based series id
    const indexedSeries = series.slice(0, MAX_NUMBER_OF_SERIES).map<InsightDataSeriesData>((series, index) => ({
        seriesId: `${index}`,
        ...series,
    }))

    // TODO(insights): inputMetadata and this function need to be re-evaluated in the future if/when support for
    // mixing series types in a single insight is possible
    function getColorForSeries(label: string, index: number): string {
        return (
            inputMetadata[`${label}-${index}`]?.stroke ||
            DATA_SERIES_COLORS_LIST[index % DATA_SERIES_COLORS_LIST.length]
        )
    }

    return indexedSeries.map((line, index) => ({
        id: line.seriesId,
        data: line.points.map(point => ({
            value: point.value,
            dateTime: new Date(point.dateTime),
            link: generateLinkURL({
                diffQuery: point.diffQuery,
            }),
        })),
        name: line.label,
        color: getColorForSeries(line.label, index),
        getLinkURL: datum => datum.link,
        getYValue: datum => datum.value,
        getXValue: datum => datum.dateTime,
    }))
}

function isGatewayTimeoutError(error: ApolloError): boolean {
    return error.networkError instanceof HTTPStatusError && error.networkError.status === 504
}
