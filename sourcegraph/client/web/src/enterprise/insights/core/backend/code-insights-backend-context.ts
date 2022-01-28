import React from 'react'
import { throwError } from 'rxjs'
import { LineChartContent, PieChartContent } from 'sourcegraph'

import { CodeInsightsBackend } from './code-insights-backend'
import { RepositorySuggestionData } from './code-insights-backend-types'

const errorMockMethod = (methodName: string) => () => throwError(new Error(`Implement ${methodName} method first`))

/**
 * Default context api class. Provides mock methods only.
 */
export class FakeDefaultCodeInsightsBackend implements CodeInsightsBackend {
    // Insights
    public getInsights = errorMockMethod('getInsights')
    public getInsightById = errorMockMethod('getInsightById')
    public findInsightByName = errorMockMethod('findInsightByName')
    public hasInsights = errorMockMethod('hasInsight')
    public getReachableInsights = errorMockMethod('getReachableInsights')
    public getBackendInsightData = errorMockMethod('getBackendInsightData')
    public getBuiltInInsightData = errorMockMethod('getBuiltInInsightData')
    public getInsightSubjects = errorMockMethod('getInsightSubjects')
    public getSubjectSettingsById = errorMockMethod('getSubjectSettingsById')
    public createInsight = errorMockMethod('createInsight')
    public createInsightWithNewFilters = errorMockMethod('createInsightWithNewFilters')
    public updateInsight = errorMockMethod('updateInsight')
    public deleteInsight = errorMockMethod('deleteInsight')

    // Dashboards
    public getDashboards = errorMockMethod('getDashboards')
    public getDashboardById = errorMockMethod('getDashboardById')
    public getDashboardSubjects = errorMockMethod('getDashboardSubjects')
    public findDashboardByName = errorMockMethod('findDashboardByName')
    public createDashboard = errorMockMethod('createDashboard')
    public deleteDashboard = errorMockMethod('deleteDashboard')
    public updateDashboard = errorMockMethod('updateDashboard')
    public assignInsightsToDashboard = errorMockMethod('assignInsightsToDashboard')

    // Live preview fetchers
    public getSearchInsightContent = (): Promise<LineChartContent<any, string>> =>
        errorMockMethod('getSearchInsightContent')().toPromise()
    public getLangStatsInsightContent = (): Promise<PieChartContent<any>> =>
        errorMockMethod('getLangStatsInsightContent')().toPromise()

    public getCaptureInsightContent = (): Promise<LineChartContent<any, string>> =>
        errorMockMethod('getCaptureInsightContent')().toPromise()

    // Repositories API
    public getRepositorySuggestions = (): Promise<RepositorySuggestionData[]> =>
        errorMockMethod('getRepositorySuggestions')().toPromise()
    public getResolvedSearchRepositories = (): Promise<string[]> =>
        errorMockMethod('getResolvedSearchRepositories')().toPromise()
}

export const CodeInsightsBackendContext = React.createContext<CodeInsightsBackend>(new FakeDefaultCodeInsightsBackend())
