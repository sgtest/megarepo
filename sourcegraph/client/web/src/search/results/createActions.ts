import { mdiMagnify } from '@mdi/js'

import { SearchPatternType } from '@sourcegraph/search'
import { FilterType } from '@sourcegraph/shared/src/search/query/filters'
import { FilterKind, findFilter } from '@sourcegraph/shared/src/search/query/query'
import { omitFilter } from '@sourcegraph/shared/src/search/query/transformer'
import { IconType } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../auth'
import { BatchChangesIcon } from '../../batches/icons'
import { CodeMonitoringLogo } from '../../code-monitoring/CodeMonitoringLogo'
import { CodeInsightsIcon } from '../../insights/Icons'

export interface CreateAction {
    url: string
    icon: IconType
    label: string
    tooltip: string
    eventToLog?: string
}

export function getSearchContextCreateAction(
    query: string | undefined,
    authenticatedUser: Pick<AuthenticatedUser, 'id'> | null
): CreateAction | null {
    if (!query || !authenticatedUser) {
        return null
    }

    const contextFilter = findFilter(query, FilterType.context, FilterKind.Global)
    if (!contextFilter || contextFilter.value?.value !== 'global') {
        return null
    }

    const sanitizedQuery = omitFilter(query, contextFilter)
    const searchParameters = new URLSearchParams()
    searchParameters.set('q', sanitizedQuery)
    const url = `/contexts/new?${searchParameters.toString()}`

    return { url, icon: mdiMagnify, label: 'Create Context', tooltip: 'Create a search context based on this query' }
}

export function getInsightsCreateAction(
    query: string | undefined,
    patternType: SearchPatternType,
    authenticatedUser: Pick<AuthenticatedUser, 'id'> | null,
    enableCodeInsights: boolean | undefined
): CreateAction | null {
    if (!enableCodeInsights || !query || !authenticatedUser) {
        return null
    }

    const searchParameters = new URLSearchParams()
    searchParameters.set('query', `${query} patterntype:${patternType}`)
    const url = `/insights/create/search?${searchParameters.toString()}`

    return {
        url,
        icon: CodeInsightsIcon,
        label: 'Create Insight',
        tooltip: 'Create Insight based on this search query',
    }
}

export function getCodeMonitoringCreateAction(
    query: string | undefined,
    patternType: SearchPatternType,
    enableCodeMonitoring: boolean
): CreateAction | null {
    if (!enableCodeMonitoring || !query) {
        return null
    }
    const searchParameters = new URLSearchParams(location.search)
    searchParameters.set('trigger-query', `${query} patterntype:${patternType}`)
    const url = `/code-monitoring/new?${searchParameters.toString()}`

    return {
        url,
        icon: CodeMonitoringLogo,
        label: 'Monitor',
        tooltip: 'Create a code monitor based on this query',
    }
}

export function getBatchChangeCreateAction(
    query: string | undefined,
    patternType: SearchPatternType,
    authenticatedUser: Pick<AuthenticatedUser, 'id'> | null,
    isServerSideBatchChangeEnabled: boolean | undefined
): CreateAction | null {
    if (!isServerSideBatchChangeEnabled || !query || !authenticatedUser) {
        return null
    }
    const searchParameters = new URLSearchParams(location.search)
    searchParameters.set('trigger-query', `${query} patterntype:${patternType}`)
    const url = `/batch-changes/create?${searchParameters.toString()}`

    return {
        url,
        icon: BatchChangesIcon,
        label: 'Create Batch Change',
        tooltip: 'Create a batch change based on this query',
        eventToLog: 'search_result_page:create_batch_change:clicked',
    }
}
