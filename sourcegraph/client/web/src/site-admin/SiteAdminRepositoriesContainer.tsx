import React, { useEffect, useMemo, useState } from 'react'

import { isEqual } from 'lodash'
import { useLocation, useNavigate } from 'react-router-dom'

import { useQuery } from '@sourcegraph/http-client'
import { Container, ErrorAlert, Input, LoadingSpinner, PageSwitcher } from '@sourcegraph/wildcard'

import { EXTERNAL_SERVICE_IDS_AND_NAMES } from '../components/externalServices/backend'
import {
    buildFilterArgs,
    FilterControl,
    FilteredConnectionFilter,
    FilteredConnectionFilterValue,
} from '../components/FilteredConnection'
import { usePageSwitcherPagination } from '../components/FilteredConnection/hooks/usePageSwitcherPagination'
import { getFilterFromURL, getUrlQuery } from '../components/FilteredConnection/utils'
import { useFeatureFlag } from '../featureFlags/useFeatureFlag'
import {
    ExternalServiceIDsAndNamesResult,
    ExternalServiceIDsAndNamesVariables,
    RepositoriesResult,
    RepositoriesVariables,
    RepositoryOrderBy,
    SiteAdminRepositoryFields,
    StatusAndRepoStatsResult,
} from '../graphql-operations'
import { PageRoutes } from '../routes.constants'

import { ValueLegendList, ValueLegendListProps } from './analytics/components/ValueLegendList'
import { REPOSITORIES_QUERY, REPO_PAGE_POLL_INTERVAL, STATUS_AND_REPO_STATS } from './backend'
import { RepositoryNode } from './RepositoryNode'

import styles from './SiteAdminRepositoriesContainer.module.scss'

const STATUS_FILTERS: { [label: string]: FilteredConnectionFilterValue } = {
    All: {
        label: 'All',
        value: 'all',
        tooltip: 'Show all repositories',
        args: {},
    },
    Cloned: {
        label: 'Cloned',
        value: 'cloned',
        tooltip: 'Show cloned repositories only',
        args: { cloneStatus: 'CLONED' },
    },
    Cloning: {
        label: 'Cloning',
        value: 'cloning',
        tooltip: 'Show repositories currently being cloned only',
        args: { cloneStatus: 'CLONING' },
    },
    NotCloned: {
        label: 'Not cloned',
        value: 'not-cloned',
        tooltip: 'Show only repositories that have not been cloned yet',
        args: { cloneStatus: 'NOT_CLONED' },
    },
    Indexed: {
        label: 'Indexed',
        value: 'indexed',
        tooltip: 'Show only repositories that have already been indexed',
        args: { notIndexed: false },
    },
    NeedsIndex: {
        label: 'Needs index',
        value: 'needs-index',
        tooltip: 'Show only repositories that need to be indexed',
        args: { indexed: false },
    },
    FailedFetchOrClone: {
        label: 'Failed fetch/clone',
        value: 'failed-fetch',
        tooltip: 'Show only repositories that have failed to fetch or clone',
        args: { failedFetch: true },
    },
    Corrupted: {
        label: 'Corrupted',
        value: 'corrupted',
        tooltip: 'Show only repositories which are corrupt',
        args: { corrupted: true },
    },
}

const FILTERS: FilteredConnectionFilter[] = [
    {
        id: 'order',
        label: 'Order',
        type: 'select',
        values: [
            {
                label: 'Name (A-Z)',
                value: 'name-asc',
                tooltip: 'Order repositories by name in ascending order',
                args: {
                    orderBy: RepositoryOrderBy.REPOSITORY_NAME,
                    descending: false,
                },
            },
            {
                label: 'Name (Z-A)',
                value: 'name-desc',
                tooltip: 'Order repositories by name in descending order',
                args: {
                    orderBy: RepositoryOrderBy.REPOSITORY_NAME,
                    descending: true,
                },
            },
            {
                label: 'Size (largest first)',
                value: 'size-desc',
                tooltip: 'Order repositories by size in descending order',
                args: {
                    orderBy: RepositoryOrderBy.SIZE,
                    descending: true,
                },
            },
            {
                label: 'Size (smallest first)',
                value: 'size-asc',
                tooltip: 'Order repositories by size in ascending order',
                args: {
                    orderBy: RepositoryOrderBy.SIZE,
                    descending: false,
                },
            },
        ],
    },
    {
        id: 'status',
        label: 'Status',
        type: 'select',
        values: Object.values(STATUS_FILTERS),
    },
]

export const SiteAdminRepositoriesContainer: React.FunctionComponent<{ alwaysPoll?: boolean }> = ({
    alwaysPoll = false,
}) => {
    const {
        data,
        loading: repoStatsLoading,
        error: repoStatsError,
        startPolling,
        stopPolling,
    } = useQuery<StatusAndRepoStatsResult>(STATUS_AND_REPO_STATS, {})
    const location = useLocation()
    const navigate = useNavigate()
    const [displayCloneProgress] = useFeatureFlag('clone-progress-logging')

    useEffect(() => {
        if (alwaysPoll || data?.repositoryStats?.total === 0 || data?.repositoryStats?.cloning !== 0) {
            startPolling(REPO_PAGE_POLL_INTERVAL)
        } else {
            stopPolling()
        }
    }, [alwaysPoll, data, startPolling, stopPolling])

    const {
        loading: extSvcLoading,
        data: extSvcs,
        error: extSvcError,
    } = useQuery<ExternalServiceIDsAndNamesResult, ExternalServiceIDsAndNamesVariables>(
        EXTERNAL_SERVICE_IDS_AND_NAMES,
        {}
    )

    const filters = useMemo(() => {
        if (!extSvcs) {
            return FILTERS
        }

        const filtersWithExternalServices = FILTERS.slice() // use slice to copy array
        if (location.pathname !== PageRoutes.SetupWizard) {
            const values = [
                {
                    label: 'All',
                    value: 'all',
                    tooltip: 'Show all repositories',
                    args: {},
                },
            ]

            for (const extSvc of extSvcs.externalServices.nodes) {
                values.push({
                    label: extSvc.displayName,
                    value: extSvc.id,
                    tooltip: `Show all repositories discovered on ${extSvc.displayName}`,
                    args: { externalService: extSvc.id },
                })
            }
            filtersWithExternalServices.push({
                id: 'codeHost',
                label: 'Code Host',
                type: 'select',
                values,
            })
        }
        return filtersWithExternalServices
    }, [extSvcs, location.pathname])

    const [filterValues, setFilterValues] = useState<Map<string, FilteredConnectionFilterValue>>(() =>
        getFilterFromURL(new URLSearchParams(location.search), filters)
    )

    useEffect(() => {
        setFilterValues(getFilterFromURL(new URLSearchParams(location.search), filters))
    }, [filters, location])

    const [searchQuery, setSearchQuery] = useState<string>(
        () => new URLSearchParams(location.search).get('query') || ''
    )

    useEffect(() => {
        const searchFragment = getUrlQuery({
            query: searchQuery,
            filters,
            filterValues,
            search: location.search,
        })
        const searchFragmentParams = new URLSearchParams(searchFragment)
        searchFragmentParams.sort()

        const oldParams = new URLSearchParams(location.search)
        oldParams.sort()

        if (!isEqual(Array.from(searchFragmentParams), Array.from(oldParams))) {
            navigate(
                {
                    search: searchFragment,
                    hash: location.hash,
                },
                {
                    replace: true,
                    // Do not throw away flash messages
                    state: location.state,
                }
            )
        }
    }, [filters, filterValues, searchQuery, location, navigate])

    const variables = useMemo<RepositoriesVariables>(() => {
        const args = buildFilterArgs(filterValues)

        return {
            ...args,
            query: searchQuery,
            indexed: args.indexed ?? true,
            notIndexed: args.notIndexed ?? true,
            failedFetch: args.failedFetch ?? false,
            corrupted: args.corrupted ?? false,
            cloneStatus: args.cloneStatus ?? null,
            externalService: args.externalService ?? null,
            displayCloneProgress,
        } as RepositoriesVariables
    }, [searchQuery, filterValues, displayCloneProgress])

    const {
        connection,
        loading: reposLoading,
        error: reposError,
        refetch,
        ...paginationProps
    } = usePageSwitcherPagination<RepositoriesResult, RepositoriesVariables, SiteAdminRepositoryFields>({
        query: REPOSITORIES_QUERY,
        variables,
        getConnection: ({ data }) => data?.repositories || undefined,
        options: { pollInterval: 5000 },
    })

    useEffect(() => {
        refetch(variables)
    }, [refetch, variables])

    const error = repoStatsError || extSvcError || reposError
    const loading = repoStatsLoading || extSvcLoading || reposLoading

    const legends = useMemo((): ValueLegendListProps['items'] | undefined => {
        if (!data) {
            return undefined
        }
        const items: ValueLegendListProps['items'] = [
            {
                value: data.repositoryStats.total,
                description: 'Repositories',
                color: 'var(--purple)',
                tooltip:
                    'Total number of repositories in the Sourcegraph instance. This number might be higher than the total number of repositories in the list below in case repository permissions do not allow you to view some repositories.',
            },
            {
                value: data.repositoryStats.notCloned,
                description: 'Queued',
                color: 'var(--body-color)',
                position: 'right',
                tooltip: 'The number of repositories that are queued to be cloned.',
                onClick: () =>
                    setFilterValues(values => {
                        const newValues = new Map(values)
                        newValues.set('status', STATUS_FILTERS.NotCloned)
                        return newValues
                    }),
            },
            {
                value: data.repositoryStats.cloning,
                description: 'Cloning',
                color: data.repositoryStats.cloning > 0 ? 'var(--primary)' : 'var(--body-color)',
                position: 'right',
                tooltip: 'The number of repositories that are currently being cloned.',
                onClick: () =>
                    setFilterValues(values => {
                        const newValues = new Map(values)
                        newValues.set('status', STATUS_FILTERS.Cloning)
                        return newValues
                    }),
            },
            {
                value: data.repositoryStats.cloned,
                description: 'Cloned',
                color: 'var(--success)',
                position: 'right',
                tooltip: 'The number of repositories that have been cloned.',
                onClick: () =>
                    setFilterValues(values => {
                        const newValues = new Map(values)
                        newValues.set('status', STATUS_FILTERS.Cloned)
                        return newValues
                    }),
            },
            {
                value: data.repositoryStats.indexed,
                description: 'Indexed',
                color: 'var(--body-color)',
                position: 'right',
                tooltip: 'The number of repositories that have been indexed for search.',
                onClick: () =>
                    setFilterValues(values => {
                        const newValues = new Map(values)
                        newValues.set('status', STATUS_FILTERS.Indexed)
                        return newValues
                    }),
            },
            {
                value: data.repositoryStats.failedFetch,
                description: 'Failed',
                color: data.repositoryStats.failedFetch > 0 ? 'var(--warning)' : 'var(--body-color)',
                position: 'right',
                tooltip: 'The number of repositories where the last syncing attempt produced an error.',
                onClick: () =>
                    setFilterValues(values => {
                        const newValues = new Map(values)
                        newValues.set('status', STATUS_FILTERS.FailedFetchOrClone)
                        return newValues
                    }),
            },
        ]
        if (data.repositoryStats.corrupted > 0) {
            items.push({
                value: data.repositoryStats.corrupted,
                description: 'Corrupted',
                color: 'var(--danger)',
                position: 'right',
                tooltip:
                    'The number of repositories where corruption has been detected. Reclone these repositories to get rid of corruption.',
                onClick: () =>
                    setFilterValues(values => {
                        const newValues = new Map(values)
                        newValues.set('status', STATUS_FILTERS.Corrupted)
                        return newValues
                    }),
            })
        }
        if (loading && !error) {
            items.splice(0, 1, {
                value: <LoadingSpinner />,
                description: 'Repositories',
            })
        }
        return items
    }, [data, setFilterValues, loading, error])

    return (
        <>
            <Container className="py-3 mb-1">
                {error && !loading && <ErrorAlert error={error} />}
                {legends && <ValueLegendList items={legends} className={styles.legend} />}
            </Container>
            {extSvcs && (
                <Container>
                    <div className="d-flex flex-sm-row flex-column-reverse justify-content-center">
                        <FilterControl
                            filters={filters}
                            values={filterValues}
                            onValueSelect={(filter: FilteredConnectionFilter, value: FilteredConnectionFilterValue) =>
                                setFilterValues(values => {
                                    const newValues = new Map(values)
                                    newValues.set(filter.id, value)
                                    return newValues
                                })
                            }
                        />
                        <Input
                            type="search"
                            className="flex-1 md-ml-5 mb-1"
                            placeholder="Search repositories..."
                            name="query"
                            value={searchQuery}
                            onChange={event => setSearchQuery(event.currentTarget.value)}
                            autoComplete="off"
                            autoCorrect="off"
                            autoCapitalize="off"
                            spellCheck={false}
                            aria-label="Search repositories..."
                            variant="regular"
                        />
                    </div>
                    <ul className="list-group list-group-flush mt-4">
                        {(connection?.nodes || []).map(node => (
                            <RepositoryNode key={node.id} node={node} />
                        ))}
                    </ul>
                    <PageSwitcher
                        {...paginationProps}
                        className="mt-4"
                        totalCount={connection?.totalCount ?? null}
                        totalLabel="repositories"
                    />
                </Container>
            )}
        </>
    )
}
