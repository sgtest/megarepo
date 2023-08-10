import { useEffect, useMemo, useState } from 'react'

import { mdiBlockHelper, mdiCog, mdiDotsHorizontal } from '@mdi/js'
import { isEqual } from 'lodash'
import { useLocation, useNavigate } from 'react-router-dom'

import { dataOrThrowErrors, useQuery } from '@sourcegraph/http-client'
import { RepoLink } from '@sourcegraph/shared/src/components/RepoLink'
import type { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import {
    Container,
    ErrorAlert,
    Link,
    LoadingSpinner,
    PageHeader,
    Input,
    useDebounce,
    Button,
    Alert,
    Text,
    Code,
    Icon,
    Menu,
    MenuButton,
    MenuList,
    MenuItem,
    MenuLink,
    Position,
} from '@sourcegraph/wildcard'

import { externalRepoIcon } from '../components/externalServices/externalServices'
import {
    buildFilterArgs,
    FilterControl,
    type FilteredConnectionFilter,
    type FilteredConnectionFilterValue,
} from '../components/FilteredConnection'
import { useShowMorePagination } from '../components/FilteredConnection/hooks/useShowMorePagination'
import { ConnectionSummary } from '../components/FilteredConnection/ui'
import { getFilterFromURL, getUrlQuery } from '../components/FilteredConnection/utils'
import { PageTitle } from '../components/PageTitle'
import type {
    PackagesResult,
    PackagesVariables,
    SiteAdminPackageFields,
    ExternalServiceKindsVariables,
    ExternalServiceKindsResult,
} from '../graphql-operations'

import { EXTERNAL_SERVICE_KINDS, PACKAGES_QUERY } from './backend'
import { RepoMirrorInfo } from './components/RepoMirrorInfo'
import { AddFilterModal } from './packages/AddFilterModal'
import { ExternalServicePackageMap } from './packages/constants'
import { ManageFiltersModal } from './packages/ManageFiltersModal'

import styles from './SiteAdminPackagesPage.module.scss'

interface PackageNodeProps {
    node: SiteAdminPackageFields
    setFilterPackage: (node: SiteAdminPackageFields) => void
}

const PackageNode: React.FunctionComponent<React.PropsWithChildren<PackageNodeProps>> = ({
    node,
    setFilterPackage,
}) => {
    const PackageIconComponent = externalRepoIcon({ serviceType: node.kind })

    const packageRepository = node.repository

    return (
        <li className="list-group-item px-0 py-2">
            <div>
                <div className={styles.node}>
                    <div>
                        <Icon as={PackageIconComponent} aria-label="Package host logo" className="mr-2" />
                        {node.blocked ? (
                            <>
                                <span>{node.name}</span>
                                <Text className="mb-0 text-danger">
                                    <small>This package is blocked by a filter.</small>
                                </Text>
                            </>
                        ) : packageRepository ? (
                            <>
                                <RepoLink repoName={node.name} to={packageRepository.url} />
                                <RepoMirrorInfo mirrorInfo={packageRepository.mirrorInfo} />
                            </>
                        ) : (
                            <>
                                <span>{node.name}</span>
                                <Text className="mb-0 text-muted">
                                    <small>This package has not yet been synced.</small>
                                </Text>
                            </>
                        )}
                    </div>
                    <div>
                        <Menu>
                            <MenuButton outline={true} aria-label="Package action">
                                <Icon svgPath={mdiDotsHorizontal} inline={false} aria-hidden={true} />
                            </MenuButton>
                            <MenuList position={Position.bottomEnd}>
                                {packageRepository?.mirrorInfo.cloned &&
                                    !packageRepository.mirrorInfo.lastError &&
                                    !packageRepository.mirrorInfo.cloneInProgress && (
                                        <MenuLink
                                            as={Link}
                                            to={`/${packageRepository.name}/-/settings`}
                                            className="p-2"
                                        >
                                            <Icon aria-hidden={true} svgPath={mdiCog} className="mr-1" />
                                            Settings
                                        </MenuLink>
                                    )}
                                <MenuItem as={Button} onSelect={() => setFilterPackage(node)} className="p-2">
                                    <Icon aria-hidden={true} svgPath={mdiBlockHelper} className="mr-1" />
                                    Add filter
                                </MenuItem>
                            </MenuList>
                        </Menu>
                    </div>
                </div>
                {packageRepository && (
                    <div>
                        {packageRepository.mirrorInfo.lastError && (
                            <div className={styles.alertWrapper}>
                                <Alert variant="warning">
                                    <Text className="font-weight-bold">Error syncing package:</Text>
                                    <Code className={styles.alertContent}>
                                        {packageRepository.mirrorInfo.lastError.replaceAll('\r', '\n')}
                                    </Code>
                                </Alert>
                            </div>
                        )}
                        {packageRepository.mirrorInfo.isCorrupted && (
                            <div className={styles.alertWrapper}>
                                <Alert variant="danger">
                                    Package is corrupt. <Link to={`/${node.name}/-/settings/mirror`}>More details</Link>
                                </Alert>
                            </div>
                        )}
                    </div>
                )}
            </div>
        </li>
    )
}

interface SiteAdminPackagesPageProps extends TelemetryProps {}

const DEFAULT_FIRST = 15

interface PackagesModalState {
    type: 'add' | 'manage' | null
    node?: SiteAdminPackageFields
}

/**
 * A page displaying the packages on this instance.
 */
export const SiteAdminPackagesPage: React.FunctionComponent<React.PropsWithChildren<SiteAdminPackagesPageProps>> = ({
    telemetryService,
}) => {
    const location = useLocation()
    const navigate = useNavigate()
    const [modalState, setModalState] = useState<PackagesModalState>({ type: null })

    useEffect(() => {
        telemetryService.logPageView('SiteAdminPackages')
    }, [telemetryService])

    const {
        loading: extSvcLoading,
        data: extSvcs,
        error: extSvcError,
    } = useQuery<ExternalServiceKindsResult, ExternalServiceKindsVariables>(EXTERNAL_SERVICE_KINDS, {})

    const ecosystemFilterValues = useMemo<FilteredConnectionFilterValue[]>(() => {
        const values = []

        for (const extSvc of extSvcs?.externalServices.nodes ?? []) {
            const packageRepoKind = ExternalServicePackageMap[extSvc.kind]

            if (packageRepoKind) {
                values.push({
                    ...packageRepoKind,
                    args: { kind: packageRepoKind.value },
                })
            }
        }

        return values
    }, [extSvcs?.externalServices.nodes])

    const filters = useMemo<FilteredConnectionFilter[]>(
        () => [
            {
                id: 'ecosystem',
                label: 'Ecosystem',
                type: 'select',
                values: [
                    {
                        label: 'All',
                        value: 'all',
                        args: {},
                    },
                    ...ecosystemFilterValues,
                ],
            },
        ],
        [ecosystemFilterValues]
    )

    const [filterValues, setFilterValues] = useState<Map<string, FilteredConnectionFilterValue>>(() =>
        getFilterFromURL(new URLSearchParams(location.search), filters)
    )

    const [searchValue, setSearchValue] = useState<string>(
        () => new URLSearchParams(location.search).get('query') || ''
    )

    const query = useDebounce(searchValue, 200)

    useEffect(() => {
        const searchFragment = getUrlQuery({
            query: searchValue,
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
    }, [filterValues, filters, searchValue, location, navigate])

    const variables = useMemo<PackagesVariables>(() => {
        const args = buildFilterArgs(filterValues)

        return {
            name: query,
            kind: null,
            after: null,
            first: DEFAULT_FIRST,
            ...args,
        }
    }, [filterValues, query])

    const {
        connection,
        error: packagesError,
        loading: packagesLoading,
        fetchMore,
        hasNextPage,
    } = useShowMorePagination<PackagesResult, PackagesVariables, SiteAdminPackageFields>({
        query: PACKAGES_QUERY,
        variables,
        getConnection: result => {
            const data = dataOrThrowErrors(result)
            return data.packageRepoReferences
        },
        options: {
            fetchPolicy: 'cache-and-network',
            useURL: true,
        },
    })

    const error = extSvcError || packagesError
    const loading = extSvcLoading || packagesLoading

    return (
        <>
            {modalState.type === 'add' ? (
                <AddFilterModal
                    node={modalState.node}
                    filters={ecosystemFilterValues}
                    onDismiss={() => setModalState({ type: null })}
                />
            ) : modalState.type === 'manage' ? (
                <ManageFiltersModal
                    filters={ecosystemFilterValues}
                    onDismiss={() => setModalState({ type: null })}
                    onAdd={() => setModalState({ type: 'add' })}
                />
            ) : (
                <></>
            )}
            <div>
                <PageTitle title="Packages - Admin" />
                <PageHeader
                    path={[{ text: 'Packages' }]}
                    headingElement="h2"
                    description={
                        <>
                            Packages are synced from connected{' '}
                            <Link to="/site-admin/external-services">code hosts</Link>.
                        </>
                    }
                    className="mb-3"
                    actions={
                        <Button variant="secondary" onClick={() => setModalState({ type: 'manage' })}>
                            Manage package filters
                        </Button>
                    }
                />

                <Container className="mb-3">
                    {error && !loading && <ErrorAlert error={error} />}
                    <Input
                        type="search"
                        className="flex-1"
                        placeholder="Search packages..."
                        name="query"
                        value={searchValue}
                        onChange={event => setSearchValue(event.currentTarget.value)}
                        autoComplete="off"
                        autoCorrect="off"
                        autoCapitalize="off"
                        spellCheck={false}
                        aria-label="Search packages..."
                        variant="regular"
                    />
                    <div className="d-flex align-items-end justify-content-between mt-3">
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
                        {connection && (
                            <ConnectionSummary
                                connection={connection}
                                connectionQuery={query}
                                hasNextPage={hasNextPage}
                                first={DEFAULT_FIRST}
                                noun="package"
                                pluralNoun="packages"
                                className="mb-0"
                            />
                        )}
                    </div>
                    {loading && !error && <LoadingSpinner className="d-block mx-auto mt-3" />}
                    {connection?.nodes && connection.nodes.length > 0 && (
                        <ul className="list-group list-group-flush mt-2">
                            {(connection?.nodes || []).map(node => (
                                <PackageNode
                                    node={node}
                                    key={node.id}
                                    setFilterPackage={node => setModalState({ type: 'add', node })}
                                />
                            ))}
                        </ul>
                    )}
                    {connection?.nodes && connection.totalCount !== connection.nodes.length && hasNextPage && (
                        <div>
                            <Button variant="link" size="sm" onClick={fetchMore}>
                                Show more
                            </Button>
                        </div>
                    )}
                </Container>
            </div>
        </>
    )
}
