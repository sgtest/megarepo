import React, { useMemo, useRef } from 'react'

import classNames from 'classnames'
import MapSearchIcon from 'mdi-react/MapSearchIcon'
import { Routes, Route } from 'react-router-dom'

import { SiteSettingFields } from '@sourcegraph/shared/src/graphql-operations'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { PageHeader, LoadingSpinner } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../auth'
import { withAuthenticatedUser } from '../auth/withAuthenticatedUser'
import { BatchChangesProps } from '../batches'
import { RouteError } from '../components/ErrorBoundary'
import { HeroPage } from '../components/HeroPage'
import { Page } from '../components/Page'
import { useFeatureFlag } from '../featureFlags/useFeatureFlag'
import { useUserExternalAccounts } from '../hooks/useUserExternalAccounts'
import { RouteV6Descriptor } from '../util/contributions'

import {
    maintenanceGroupHeaderLabel,
    maintenanceGroupInstrumentationItemLabel,
    maintenanceGroupMonitoringItemLabel,
    maintenanceGroupMigrationsItemLabel,
    maintenanceGroupUpdatesItemLabel,
    maintenanceGroupTracingItemLabel,
} from './sidebaritems'
import { SiteAdminSidebar, SiteAdminSideBarGroups } from './SiteAdminSidebar'

import styles from './SiteAdminArea.module.scss'

const NotFoundPage: React.ComponentType<React.PropsWithChildren<{}>> = () => (
    <HeroPage
        icon={MapSearchIcon}
        title="404: Not Found"
        subtitle="Sorry, the requested site admin page was not found."
    />
)

const NotSiteAdminPage: React.ComponentType<React.PropsWithChildren<{}>> = () => (
    <HeroPage icon={MapSearchIcon} title="403: Forbidden" subtitle="Only site admins are allowed here." />
)

export interface SiteAdminAreaRouteContext
    extends PlatformContextProps,
        SettingsCascadeProps,
        BatchChangesProps,
        TelemetryProps {
    site: Pick<SiteSettingFields, '__typename' | 'id'>
    authenticatedUser: AuthenticatedUser
    isSourcegraphDotCom: boolean
    isSourcegraphApp: boolean

    /** This property is only used by {@link SiteAdminOverviewPage}. */
    overviewComponents: readonly React.ComponentType<React.PropsWithChildren<{}>>[]
}

export interface SiteAdminAreaRoute extends RouteV6Descriptor<SiteAdminAreaRouteContext> {}

interface SiteAdminAreaProps extends PlatformContextProps, SettingsCascadeProps, BatchChangesProps, TelemetryProps {
    routes: readonly SiteAdminAreaRoute[]
    sideBarGroups: SiteAdminSideBarGroups
    overviewComponents: readonly React.ComponentType<React.PropsWithChildren<unknown>>[]
    authenticatedUser: AuthenticatedUser
    isSourcegraphDotCom: boolean
    isSourcegraphApp: boolean
}

const sourcegraphOperatorSiteAdminMaintenanceBlockItems = new Set([
    maintenanceGroupInstrumentationItemLabel,
    maintenanceGroupMonitoringItemLabel,
    maintenanceGroupMigrationsItemLabel,
    maintenanceGroupUpdatesItemLabel,
    maintenanceGroupTracingItemLabel,
])

const AuthenticatedSiteAdminArea: React.FunctionComponent<React.PropsWithChildren<SiteAdminAreaProps>> = props => {
    const reference = useRef<HTMLDivElement>(null)

    const { data: externalAccounts, loading: isExternalAccountsLoading } = useUserExternalAccounts(
        props.authenticatedUser.username
    )
    const [isSourcegraphOperatorSiteAdminHideMaintenance] = useFeatureFlag(
        'sourcegraph-operator-site-admin-hide-maintenance'
    )
    const [ownAnalyticsEnabled] = useFeatureFlag('own-analytics', false)

    const adminSideBarGroups = useMemo(
        () =>
            props.sideBarGroups.map(group => {
                if (
                    !isSourcegraphOperatorSiteAdminHideMaintenance ||
                    group.header?.label !== maintenanceGroupHeaderLabel ||
                    (!isExternalAccountsLoading &&
                        externalAccounts.some(account => account.serviceType === 'sourcegraph-operator'))
                ) {
                    return group
                }

                return {
                    ...group,
                    items: group.items.filter(
                        item => !sourcegraphOperatorSiteAdminMaintenanceBlockItems.has(item.label)
                    ),
                }
            }),
        [
            props.sideBarGroups,
            isSourcegraphOperatorSiteAdminHideMaintenance,
            isExternalAccountsLoading,
            externalAccounts,
        ]
    )

    // If not site admin, redirect to sign in.
    if (!props.authenticatedUser.siteAdmin) {
        return <NotSiteAdminPage />
    }

    const context: SiteAdminAreaRouteContext = {
        authenticatedUser: props.authenticatedUser,
        platformContext: props.platformContext,
        settingsCascade: props.settingsCascade,
        isSourcegraphDotCom: props.isSourcegraphDotCom,
        isSourcegraphApp: props.isSourcegraphApp,
        batchChangesEnabled: props.batchChangesEnabled,
        batchChangesExecutionEnabled: props.batchChangesExecutionEnabled,
        batchChangesWebhookLogsEnabled: props.batchChangesWebhookLogsEnabled,
        site: { __typename: 'Site' as const, id: window.context.siteGQLID },
        overviewComponents: props.overviewComponents,
        telemetryService: props.telemetryService,
    }

    return (
        <Page>
            <PageHeader>
                <PageHeader.Heading as="h2" styleAs="h1">
                    <PageHeader.Breadcrumb>
                        {props.isSourcegraphApp ? 'Advanced Settings' : 'Admin'}
                    </PageHeader.Breadcrumb>
                </PageHeader.Heading>
            </PageHeader>
            <div className="d-flex my-3 flex-column flex-sm-row" ref={reference}>
                <SiteAdminSidebar
                    className={classNames('flex-0 mr-3 mb-4', styles.sidebar)}
                    groups={adminSideBarGroups}
                    ownAnalyticsEnabled={ownAnalyticsEnabled}
                    isSourcegraphDotCom={props.isSourcegraphDotCom}
                    isSourcegraphApp={props.isSourcegraphApp}
                    batchChangesEnabled={props.batchChangesEnabled}
                    batchChangesExecutionEnabled={props.batchChangesExecutionEnabled}
                    batchChangesWebhookLogsEnabled={props.batchChangesWebhookLogsEnabled}
                />
                <div className="flex-bounded">
                    <React.Suspense fallback={<LoadingSpinner className="m-2" />}>
                        <Routes>
                            {props.routes.map(
                                ({ render, path, condition = () => true }) =>
                                    condition(context) && (
                                        <Route
                                            // see https://github.com/ReactTraining/react-router/issues/4578#issuecomment-334489490
                                            key="hardcoded-key"
                                            errorElement={<RouteError />}
                                            path={path}
                                            element={render(context)}
                                        />
                                    )
                            )}
                            <Route path="*" element={<NotFoundPage />} />
                        </Routes>
                    </React.Suspense>
                </div>
            </div>
        </Page>
    )
}

/**
 * Renders a layout of a sidebar and a content area to display site admin information.
 */
export const SiteAdminArea = withAuthenticatedUser(AuthenticatedSiteAdminArea)
