import { of, throwError } from 'rxjs'

import { getCombinedViews, getInsightCombinedViews } from './api/get-combined-views'
import { getSearchInsightContent } from './api/get-search-insight-content'
import { getSubjectSettings, updateSubjectSettings } from './api/subject-settings'
import { ApiService } from './types'

/**
 * Main API service to get data for code insights
 * */
export const createInsightAPI = (): ApiService => ({
    getCombinedViews,
    getInsightCombinedViews,
    getSubjectSettings,
    updateSubjectSettings,
    getSearchInsightContent,
})

/**
 * Mock API service. Used to mock part or some specific api requests in demo and
 * storybook stories.
 * */
export const createMockInsightAPI = (overrideRequests: Partial<ApiService>): ApiService => ({
    getCombinedViews: () => of([]),
    getInsightCombinedViews: () => of([]),
    getSubjectSettings: () => throwError(new Error('Implement getSubjectSettings handler first')),
    updateSubjectSettings: () => throwError(new Error('Implement getSubjectSettings handler first')),
    getSearchInsightContent: () => Promise.reject(new Error('Implement getSubjectSettings handler first')),
    ...overrideRequests,
})
