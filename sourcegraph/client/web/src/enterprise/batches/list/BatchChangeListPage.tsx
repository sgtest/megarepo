import React, { useEffect, useCallback, useState, useMemo } from 'react'

import classNames from 'classnames'
import { RouteComponentProps } from 'react-router'

import { dataOrThrowErrors, useQuery } from '@sourcegraph/http-client'
import { Settings } from '@sourcegraph/shared/src/schema/settings.schema'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { PageHeader, CardBody, Card, Link, Container, H3, H2, H4 } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../../auth'
import { isBatchChangesExecutionEnabled } from '../../../batches'
import { BatchChangesIcon } from '../../../batches/icons'
import { useConnection } from '../../../components/FilteredConnection/hooks/useConnection'
import {
    ConnectionContainer,
    ConnectionError,
    ConnectionList,
    ConnectionLoading,
    ConnectionSummary,
    ShowMoreButton,
    SummaryContainer,
} from '../../../components/FilteredConnection/ui'
import { Page } from '../../../components/Page'
import {
    ListBatchChange,
    Scalars,
    BatchChangesVariables,
    BatchChangesResult,
    BatchChangesByNamespaceResult,
    BatchChangesByNamespaceVariables,
    GetLicenseAndUsageInfoResult,
    GetLicenseAndUsageInfoVariables,
} from '../../../graphql-operations'

import { BATCH_CHANGES, BATCH_CHANGES_BY_NAMESPACE, GET_LICENSE_AND_USAGE_INFO } from './backend'
import { BatchChangeListFilters } from './BatchChangeListFilters'
import { BatchChangeNode } from './BatchChangeNode'
import { BatchChangesListIntro } from './BatchChangesListIntro'
import { GettingStarted } from './GettingStarted'
import { NewBatchChangeButton } from './NewBatchChangeButton'
import { useBatchChangeListFilters } from './useBatchChangeListFilters'

import styles from './BatchChangeListPage.module.scss'

export interface BatchChangeListPageProps
    extends TelemetryProps,
        Pick<RouteComponentProps, 'location'>,
        SettingsCascadeProps<Settings> {
    canCreate: boolean
    headingElement: 'h1' | 'h2'
    namespaceID?: Scalars['ID']
    /** For testing only. */
    openTab?: SelectedTab
}

type SelectedTab = 'batchChanges' | 'gettingStarted'

const BATCH_CHANGES_PER_PAGE_COUNT = 15

/**
 * A list of all batch changes on the Sourcegraph instance.
 */
export const BatchChangeListPage: React.FunctionComponent<React.PropsWithChildren<BatchChangeListPageProps>> = ({
    canCreate,
    namespaceID,
    headingElement,
    location,
    openTab,
    settingsCascade,
    telemetryService,
}) => {
    useEffect(() => telemetryService.logViewEvent('BatchChangesListPage'), [telemetryService])

    const isExecutionEnabled = isBatchChangesExecutionEnabled(settingsCascade)

    const { selectedFilters, setSelectedFilters, selectedStates } = useBatchChangeListFilters()
    const [selectedTab, setSelectedTab] = useState<SelectedTab>(openTab ?? 'batchChanges')

    // We keep state to track to the last total count of batch changes in the connection
    // to avoid the display flickering as the connection is loading more data or a
    // different set of filtered data.
    const [lastTotalCount, setLastTotalCount] = useState<number>(0)

    // We use the license and usage query to check whether or not there are any batch
    // changes _at all_. If there aren't, we automatically switch the user to the "Getting
    // started" tab.
    const onUsageCheckCompleted = useCallback(
        (data: GetLicenseAndUsageInfoResult) => {
            if (!openTab && data.allBatchChanges.totalCount === 0) {
                setSelectedTab('gettingStarted')
            }
        },
        [openTab]
    )

    const { data: licenseAndUsageInfo } = useQuery<GetLicenseAndUsageInfoResult, GetLicenseAndUsageInfoVariables>(
        GET_LICENSE_AND_USAGE_INFO,
        { onCompleted: onUsageCheckCompleted }
    )

    const { connection, error, loading, fetchMore, hasNextPage } = useConnection<
        BatchChangesByNamespaceResult | BatchChangesResult,
        BatchChangesByNamespaceVariables | BatchChangesVariables,
        ListBatchChange
    >({
        query: namespaceID ? BATCH_CHANGES_BY_NAMESPACE : BATCH_CHANGES,
        variables: {
            namespaceID,
            states: selectedStates,
            first: BATCH_CHANGES_PER_PAGE_COUNT,
            after: null,
            viewerCanAdminister: null,
        },
        options: { useURL: true },
        getConnection: result => {
            const data = dataOrThrowErrors(result)
            if (!namespaceID) {
                return (data as BatchChangesResult).batchChanges
            }
            if (!('node' in data) || !data.node) {
                throw new Error('Namespace not found')
            }
            if (data.node.__typename !== 'Org' && data.node.__typename !== 'User') {
                throw new Error(`Requested node is a ${data.node.__typename}, not a User or Org`)
            }
            return data.node.batchChanges
        },
    })

    useEffect(() => {
        // If the data in the connection updates with new results, update the total count.
        if (connection) {
            setLastTotalCount(connection.totalCount || 0)
        }
    }, [connection])

    return (
        <Page>
            <PageHeader
                path={[{ icon: BatchChangesIcon, text: 'Batch Changes' }]}
                className="test-batches-list-page mb-3"
                actions={canCreate ? <NewBatchChangeButton to={`${location.pathname}/create`} /> : null}
                headingElement={headingElement}
                description="Run custom code over hundreds of repositories and manage the resulting changesets."
            />
            <BatchChangesListIntro isLicensed={licenseAndUsageInfo?.batchChanges || licenseAndUsageInfo?.campaigns} />
            <BatchChangeListTabHeader selectedTab={selectedTab} setSelectedTab={setSelectedTab} />
            {selectedTab === 'gettingStarted' && <GettingStarted className="mb-4" footer={<GettingStartedFooter />} />}
            {selectedTab === 'batchChanges' && (
                <Container className="mb-4">
                    <ConnectionContainer>
                        <div className={styles.filtersRow}>
                            {(licenseAndUsageInfo?.allBatchChanges.totalCount || 0) > 0 && (
                                <H3 as={H2} className="align-self-end flex-1">
                                    {lastTotalCount} batch changes
                                </H3>
                            )}
                            <H4 as={H3} className="mb-0 mr-2">
                                Status
                            </H4>
                            <BatchChangeListFilters
                                className="m-0"
                                isExecutionEnabled={isExecutionEnabled}
                                value={selectedFilters}
                                onChange={setSelectedFilters}
                            />
                        </div>
                        {error && <ConnectionError errors={[error.message]} />}
                        <ConnectionList
                            as="div"
                            className={classNames(styles.grid, isExecutionEnabled ? styles.wide : styles.narrow)}
                        >
                            {connection?.nodes?.map(node => (
                                <BatchChangeNode
                                    key={node.id}
                                    node={node}
                                    isExecutionEnabled={isExecutionEnabled}
                                    // Show the namespace unless we're viewing batch changes for a single namespace.
                                    displayNamespace={!namespaceID}
                                />
                            ))}
                        </ConnectionList>
                        {loading && <ConnectionLoading />}
                        {connection && (
                            <SummaryContainer centered={true}>
                                <ConnectionSummary
                                    noSummaryIfAllNodesVisible={true}
                                    first={BATCH_CHANGES_PER_PAGE_COUNT}
                                    connection={connection}
                                    noun="batch change"
                                    pluralNoun="batch changes"
                                    hasNextPage={hasNextPage}
                                    emptyElement={
                                        <BatchChangeListEmptyElement canCreate={canCreate} location={location} />
                                    }
                                />
                                {hasNextPage && <ShowMoreButton onClick={fetchMore} />}
                            </SummaryContainer>
                        )}
                    </ConnectionContainer>
                </Container>
            )}
        </Page>
    )
}

export interface NamespaceBatchChangeListPageProps extends Omit<BatchChangeListPageProps, 'canCreate'> {
    authenticatedUser: AuthenticatedUser
    namespaceID: Scalars['ID']
}

/**
 * A list of all batch changes in a namespace.
 */
export const NamespaceBatchChangeListPage: React.FunctionComponent<
    React.PropsWithChildren<NamespaceBatchChangeListPageProps>
> = ({ authenticatedUser, namespaceID, ...props }) => {
    // A user should only see the button to create a batch change in a namespace if it is
    // their namespace (user namespace), or they belong to it (organization namespace)
    const canCreateInThisNamespace = useMemo(
        () =>
            authenticatedUser.id === namespaceID ||
            authenticatedUser.organizations.nodes.map(org => org.id).includes(namespaceID),
        [authenticatedUser, namespaceID]
    )

    return <BatchChangeListPage {...props} canCreate={canCreateInThisNamespace} namespaceID={namespaceID} />
}

interface BatchChangeListEmptyElementProps extends Pick<BatchChangeListPageProps, 'location' | 'canCreate'> {}

const BatchChangeListEmptyElement: React.FunctionComponent<
    React.PropsWithChildren<BatchChangeListEmptyElementProps>
> = ({ canCreate, location }) => (
    <div className="w-100 py-5 text-center">
        <p>
            <strong>No batch changes have been created.</strong>
        </p>
        {canCreate ? <NewBatchChangeButton to={`${location.pathname}/create`} /> : null}
    </div>
)

const BatchChangeListTabHeader: React.FunctionComponent<
    React.PropsWithChildren<{
        selectedTab: SelectedTab
        setSelectedTab: (selectedTab: SelectedTab) => void
    }>
> = ({ selectedTab, setSelectedTab }) => {
    const onSelectBatchChanges = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setSelectedTab('batchChanges')
        },
        [setSelectedTab]
    )
    const onSelectGettingStarted = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setSelectedTab('gettingStarted')
        },
        [setSelectedTab]
    )
    return (
        <div className="overflow-auto mb-2">
            <ul className="nav nav-tabs d-inline-flex d-sm-flex flex-nowrap text-nowrap">
                <li className="nav-item">
                    <Link
                        to=""
                        onClick={onSelectBatchChanges}
                        className={classNames('nav-link', selectedTab === 'batchChanges' && 'active')}
                        role="button"
                    >
                        <span className="text-content" data-tab-content="All batch changes">
                            All batch changes
                        </span>
                    </Link>
                </li>
                <li className="nav-item">
                    <Link
                        to=""
                        onClick={onSelectGettingStarted}
                        className={classNames('nav-link', selectedTab === 'gettingStarted' && 'active')}
                        role="button"
                        data-testid="test-getting-started-btn"
                    >
                        <span className="text-content" data-tab-content="Getting started">
                            Getting started
                        </span>
                    </Link>
                </li>
            </ul>
        </div>
    )
}

const GettingStartedFooter: React.FunctionComponent<React.PropsWithChildren<{}>> = () => (
    <div className="row">
        <div className="col-12 col-sm-8 offset-sm-2 col-md-6 offset-md-3">
            <Card>
                <CardBody className="text-center">
                    <p>Create your first batch change</p>
                    <h2 className="mb-0">
                        <Link to="/help/batch_changes/quickstart" target="_blank" rel="noopener">
                            Batch Changes quickstart
                        </Link>
                    </h2>
                </CardBody>
            </Card>
        </div>
    </div>
)
