import React, { useCallback, useEffect, useMemo, useState } from 'react'

import { mdiBookOutline } from '@mdi/js'
import classNames from 'classnames'
import { type Location, Navigate, useNavigate, useLocation, type NavigateFunction } from 'react-router-dom'
import type { Observable } from 'rxjs'
import { catchError, startWith, switchMap } from 'rxjs/operators'

import { asError, type ErrorLike, isErrorLike } from '@sourcegraph/common'
import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary/useTemporarySetting'
import type { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { PageHeader, Button, useEventObservable, Alert, ButtonLink } from '@sourcegraph/wildcard'

import type { AuthenticatedUser } from '../../auth'
import type { FilteredConnectionFilter } from '../../components/FilteredConnection'
import { Page } from '../../components/Page'
import { type CreateNotebookVariables, NotebooksOrderBy } from '../../graphql-operations'
import { EnterprisePageRoutes } from '../../routes.constants'
import { fetchNotebooks as _fetchNotebooks, createNotebook as _createNotebook } from '../backend'

import { NotebooksGettingStartedTab } from './NotebooksGettingStartedTab'
import { NotebooksList, type NotebooksListProps } from './NotebooksList'
import { NotebooksListPageHeader } from './NotebooksListPageHeader'

export interface NotebooksListPageProps extends TelemetryProps {
    authenticatedUser: AuthenticatedUser | null
    fetchNotebooks?: typeof _fetchNotebooks
    createNotebook?: typeof _createNotebook
}

type NotebooksTab = 'notebooks' | 'getting-started'

type Tabs = { tab: NotebooksTab; title: string; isActive: boolean; logEventName: string }[]

function getSelectedTabFromLocation(locationSearch: string, authenticatedUser: AuthenticatedUser | null): NotebooksTab {
    const urlParameters = new URLSearchParams(locationSearch)
    switch (urlParameters.get('tab')) {
        case 'notebooks':
            return 'notebooks'
        case 'getting-started':
            return 'getting-started'
    }
    return authenticatedUser ? 'notebooks' : 'getting-started'
}

function setSelectedLocationTab(location: Location, navigate: NavigateFunction, selectedTab: NotebooksTab): void {
    const urlParameters = new URLSearchParams(location.search)
    urlParameters.set('tab', selectedTab)
    if (location.search !== urlParameters.toString()) {
        navigate({ ...location, search: urlParameters.toString() }, { replace: true })
    }
}

const LOADING = 'loading' as const

interface NotebooksFilter extends Pick<NotebooksListProps, 'creatorUserID' | 'starredByUserID' | 'namespace'> {
    id: string
    label: string
    logEventName: string
}

export const NotebooksListPage: React.FunctionComponent<React.PropsWithChildren<NotebooksListPageProps>> = ({
    authenticatedUser,
    telemetryService,
    fetchNotebooks = _fetchNotebooks,
    createNotebook = _createNotebook,
}) => {
    useEffect(() => {
        telemetryService.logPageView('SearchNotebooksListPage')
    }, [telemetryService])

    const [importState, setImportState] = useState<typeof LOADING | ErrorLike | undefined>()
    const navigate = useNavigate()
    const location = useLocation()

    const [selectedTab, setSelectedTab] = useState<NotebooksTab>(
        getSelectedTabFromLocation(location.search, authenticatedUser)
    )
    const [selectedFilter, setSelectedFilter] = useState<NotebooksFilter>()

    const [hasSeenGettingStartedTab] = useTemporarySetting('search.notebooks.gettingStartedTabSeen', false)

    useEffect(() => {
        if (typeof hasSeenGettingStartedTab !== 'undefined' && !hasSeenGettingStartedTab) {
            setSelectedTab('getting-started')
        }
    }, [hasSeenGettingStartedTab, setSelectedTab])

    const onSelectTab = useCallback(
        (tab: NotebooksTab, logName: string) => {
            setSelectedTab(tab)
            setSelectedLocationTab(location, navigate, tab)
            telemetryService.log(logName)
        },
        [navigate, location, setSelectedTab, telemetryService]
    )

    const orderOptions: FilteredConnectionFilter[] = [
        {
            label: 'Order by',
            type: 'select',
            id: 'order',
            tooltip: 'Order notebooks',
            values: [
                {
                    value: 'updated-at-desc',
                    label: 'Last update (descending)',
                    args: {
                        orderBy: NotebooksOrderBy.NOTEBOOK_UPDATED_AT,
                        descending: true,
                    },
                },
                {
                    value: 'updated-at-asc',
                    label: 'Last update (ascending)',
                    args: {
                        orderBy: NotebooksOrderBy.NOTEBOOK_UPDATED_AT,
                        descending: false,
                    },
                },
                {
                    value: 'stars-desc',
                    label: 'Stars (descending)',
                    args: {
                        orderBy: NotebooksOrderBy.NOTEBOOK_STAR_COUNT,
                        descending: true,
                    },
                },
                {
                    value: 'stars-asc',
                    label: 'Stars (ascending)',
                    args: {
                        orderBy: NotebooksOrderBy.NOTEBOOK_STAR_COUNT,
                        descending: false,
                    },
                },
                {
                    value: 'created-at-desc',
                    label: 'Creation date (descending)',
                    args: {
                        orderBy: NotebooksOrderBy.NOTEBOOK_CREATED_AT,
                        descending: true,
                    },
                },
                {
                    value: 'created-at-asc',
                    label: 'Creation date (ascending)',
                    args: {
                        orderBy: NotebooksOrderBy.NOTEBOOK_CREATED_AT,
                        descending: false,
                    },
                },
            ],
        },
    ]

    const orgFilters: NotebooksFilter[] | undefined = useMemo(
        () =>
            authenticatedUser?.organizations.nodes.map(org => ({
                id: `org-${org.id}-notebooks`,
                label: `${org.displayName} notebooks`,
                logEventName: 'OrgNotebooks',
                namespace: org.id,
            })),
        [authenticatedUser]
    )

    const tabs: Tabs = useMemo(
        () => [
            {
                tab: 'notebooks',
                title: 'Notebooks',
                isActive: selectedTab === 'notebooks',
                logEventName: 'Notebooks',
            },
            {
                tab: 'getting-started',
                title: 'Getting Started',
                isActive: selectedTab === 'getting-started',
                logEventName: 'GettingStarted',
            },
        ],
        [selectedTab]
    )

    const filters: NotebooksFilter[] = useMemo(
        () =>
            [
                authenticatedUser && {
                    id: 'my-notebooks',
                    label: 'Created by me',
                    creatorUserID: authenticatedUser.id,
                    logEventName: 'MyNotebooks',
                },
                authenticatedUser && {
                    id: 'starred-notebooks',
                    label: 'Starred',
                    starredByUserID: authenticatedUser.id,
                    logEventName: 'StarredNotebooks',
                },
                {
                    id: 'all-notebooks',
                    label: 'All notebooks',
                    logEventName: 'ExploreNotebooks',
                },
                ...(orgFilters || []),
            ].filter((filter): filter is NotebooksFilter => !!filter),
        [authenticatedUser, orgFilters]
    )

    useEffect(() => {
        if (!selectedFilter) {
            setSelectedFilter(filters[0])
        }
    }, [selectedFilter, filters, setSelectedFilter])

    const [importNotebook, importedNotebookOrError] = useEventObservable(
        useCallback(
            (notebook: Observable<CreateNotebookVariables['notebook']>) =>
                notebook.pipe(
                    switchMap(notebook =>
                        createNotebook({ notebook }).pipe(
                            startWith(LOADING),
                            catchError(error => {
                                setImportState(asError(error))
                                return []
                            })
                        )
                    )
                ),
            [createNotebook, setImportState]
        )
    )

    if (importedNotebookOrError && importedNotebookOrError !== LOADING) {
        telemetryService.log('SearchNotebookImportedFromMarkdown')
        return <Navigate to={EnterprisePageRoutes.Notebook.replace(':id', importedNotebookOrError.id)} replace={true} />
    }

    return (
        <div className="w-100">
            <Page>
                <PageHeader
                    actions={
                        authenticatedUser && (
                            <NotebooksListPageHeader
                                authenticatedUser={authenticatedUser}
                                importNotebook={importNotebook}
                                setImportState={setImportState}
                                telemetryService={telemetryService}
                            />
                        )
                    }
                    className="mb-3"
                >
                    <PageHeader.Heading as="h2" styleAs="h1">
                        <PageHeader.Breadcrumb icon={mdiBookOutline}>Notebooks</PageHeader.Breadcrumb>
                    </PageHeader.Heading>
                </PageHeader>
                {isErrorLike(importState) && (
                    <Alert variant="danger">
                        Error while importing the notebook: <strong>{importState.message}</strong>
                    </Alert>
                )}
                <div className="mb-4">
                    <div className="nav nav-tabs">
                        {tabs.map(({ tab, title, isActive, logEventName }) => (
                            <div className="nav-item" key={tab}>
                                <ButtonLink
                                    to=""
                                    role="button"
                                    onSelect={event => {
                                        event.preventDefault()
                                        onSelectTab(tab, `SearchNotebooks${logEventName}TabClick`)
                                    }}
                                    className={classNames('nav-link', isActive && 'active')}
                                >
                                    <span className="text-content" data-tab-content={title}>
                                        {title}
                                    </span>
                                </ButtonLink>
                            </div>
                        ))}
                    </div>
                </div>

                {selectedTab === 'notebooks' && (
                    <div className="row mb-5">
                        <div className="d-flex flex-column col-sm-2">
                            {filters.map(filter => (
                                <Button
                                    key={filter.id}
                                    className="text-left"
                                    onClick={() => setSelectedFilter(filter)}
                                    variant={selectedFilter?.id === filter.id ? 'primary' : undefined}
                                >
                                    {filter.label}
                                </Button>
                            ))}
                        </div>
                        <div className="d-flex flex-column col-sm-10">
                            {selectedFilter && (
                                <NotebooksList
                                    key={selectedFilter.id}
                                    title={selectedFilter.label}
                                    logEventName={selectedFilter.logEventName}
                                    fetchNotebooks={fetchNotebooks}
                                    orderOptions={orderOptions}
                                    creatorUserID={selectedFilter.creatorUserID}
                                    starredByUserID={selectedFilter.starredByUserID}
                                    namespace={selectedFilter.namespace}
                                    telemetryService={telemetryService}
                                />
                            )}
                        </div>
                    </div>
                )}

                {selectedTab === 'getting-started' && (
                    <NotebooksGettingStartedTab
                        telemetryService={telemetryService}
                        authenticatedUser={authenticatedUser}
                    />
                )}
            </Page>
        </div>
    )
}
