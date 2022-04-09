import { Duration } from 'date-fns'
import { LineChartContent as LegacyLineChartContent } from 'sourcegraph'

import { Series } from '../../../../charts'
import {
    RuntimeInsight,
    InsightDashboard,
    SearchBasedInsightSeries,
    CaptureGroupInsight,
    LangStatsInsight,
    InsightsDashboardOwner,
    SearchBackendBasedInsight,
    SearchRuntimeBasedInsight,
} from '../types'

export interface PieChartContent<Datum> {
    data: Datum[]
    getDatumValue: (datum: Datum) => number
    getDatumName: (datum: Datum) => string
    getDatumColor: (datum: Datum) => string | undefined
    getDatumLink?: (datum: Datum) => string | undefined
}

export interface LineChartContent<Datum> {
    data: Datum[]
    series: Series<Datum>[]
    getXValue: (datum: Datum) => Date
}

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

export type MinimalSearchRuntimeBasedInsightData = Omit<
    SearchRuntimeBasedInsight,
    'id' | 'dashboardReferenceCount' | 'isFrozen'
>
export type MinimalSearchBackendBasedInsightData = Omit<
    SearchBackendBasedInsight,
    'id' | 'dashboardReferenceCount' | 'isFrozen'
>
export type MinimalSearchBasedInsightData = MinimalSearchRuntimeBasedInsightData | MinimalSearchBackendBasedInsightData

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

export interface SearchInsightSettings {
    series: SearchBasedInsightSeries[]
    step: Duration
    repositories: string[]
}

export interface LangStatsInsightsSettings {
    repository: string
    otherThreshold: number
}

export interface CaptureInsightSettings {
    repositories: string[]
    query: string
    step: Duration
}

export interface AccessibleInsightInfo {
    id: string
    title: string
}

export interface BackendInsightData {
    id: string
    view: {
        title: string
        subtitle?: string
        content: LegacyLineChartContent<any, string>[]
        isFetchingHistoricalData: boolean
    }
}

export interface GetBuiltInsightInput {
    insight: RuntimeInsight
}

export interface GetSearchInsightContentInput {
    insight: SearchInsightSettings
}

export interface GetLangStatsInsightContentInput {
    insight: LangStatsInsightsSettings
}

export interface RepositorySuggestionData {
    id: string
    name: string
}
