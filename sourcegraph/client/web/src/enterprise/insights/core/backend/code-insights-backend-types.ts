import { Duration } from 'date-fns'

import { Series } from '../../../../charts'
import { GroupByField } from '../../../../graphql-operations'
import {
    RuntimeInsight,
    InsightDashboard,
    SearchBasedInsightSeries,
    CaptureGroupInsight,
    LangStatsInsight,
    InsightsDashboardOwner,
    SearchBasedInsight,
} from '../types'
import { InsightContentType } from '../types/insight/common'

export interface CategoricalChartContent<Datum> {
    data: Datum[]
    getDatumValue: (datum: Datum) => number
    getDatumName: (datum: Datum) => string
    getDatumColor: (datum: Datum) => string | undefined
    getDatumLink?: (datum: Datum) => string | undefined
    getCategory?: (datum: Datum) => string | undefined
}

export interface SeriesChartContent<Datum> {
    series: Series<Datum>[]
}

export interface InsightCategoricalContent<Datum> {
    type: InsightContentType.Categorical
    content: CategoricalChartContent<Datum>
}

export interface InsightSeriesContent<Datum> {
    type: InsightContentType.Series
    content: SeriesChartContent<Datum>
}

export type InsightContent<Datum> = InsightSeriesContent<Datum> | InsightCategoricalContent<Datum>

export interface DashboardCreateInput {
    name: string
    owners: InsightsDashboardOwner[]
}

export interface DashboardCreateResult {
    id: string
}

export interface DashboardUpdateInput {
    nextDashboardInput: DashboardCreateInput
    id: string
}

export interface AssignInsightsToDashboardInput {
    id: string
    prevInsightIds: string[]
    nextInsightIds: string[]
}

export interface DashboardUpdateResult {
    id: string
}

export interface DashboardDeleteInput {
    id: string
}

export interface FindInsightByNameInput {
    name: string
}

export type MinimalSearchBasedInsightData = Omit<SearchBasedInsight, 'id' | 'dashboardReferenceCount' | 'isFrozen'>

export type MinimalCaptureGroupInsightData = Omit<CaptureGroupInsight, 'id' | 'dashboardReferenceCount' | 'isFrozen'>
export type MinimalLangStatsInsightData = Omit<LangStatsInsight, 'id' | 'dashboardReferenceCount' | 'isFrozen'>

export type CreationInsightInput =
    | MinimalSearchBasedInsightData
    | MinimalCaptureGroupInsightData
    | MinimalLangStatsInsightData

export interface InsightCreateInput {
    insight: CreationInsightInput
    dashboard: InsightDashboard | null
}

export interface InsightUpdateInput {
    insightId: string
    nextInsightData: CreationInsightInput
}

export interface RemoveInsightFromDashboardInput {
    insightId: string
    dashboardId: string
}

export interface CaptureInsightSettings {
    repositories: string[]
    query: string
    step: Duration
}

export interface InsightPreviewSettings {
    repositories: string[]
    step: Duration
    series: SeriesPreviewSettings[]
}

export interface SeriesPreviewSettings {
    query: string
    generatedFromCaptureGroup?: boolean
    label: string
    stroke: string
    groupBy?: GroupByField
}

export interface AccessibleInsightInfo {
    id: string
    title: string
}

export interface BackendInsightDatum {
    dateTime: Date
    value: number | null
    link?: string
}

export interface BackendInsightData {
    content: SeriesChartContent<any>
    isFetchingHistoricalData: boolean
}

export interface GetBuiltInsightInput {
    insight: RuntimeInsight
}

export interface GetSearchInsightContentInput {
    series: SearchBasedInsightSeries[]
    step: Duration
    repositories: string[]
}

export interface GetLangStatsInsightContentInput {
    repository: string
    otherThreshold: number
}

export interface RepositorySuggestionData {
    id: string
    name: string
}

export interface UiFeaturesConfig {
    licensed: boolean
    insightsLimit: number | null
}

export interface HasInsightsInput {
    first: number
    isFrozen?: boolean
}
