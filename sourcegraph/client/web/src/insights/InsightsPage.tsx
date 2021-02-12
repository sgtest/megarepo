import React, { useCallback, useEffect, useMemo } from 'react'
import { useObservable } from '../../../shared/src/util/useObservable'
import { getViewsForContainer } from '../../../shared/src/api/client/services/viewService'
import { ContributableViewContainer } from '../../../shared/src/api/protocol'
import { ExtensionsControllerProps } from '../../../shared/src/extensions/controller'
import { ViewGrid, ViewGridProps } from '../repo/tree/ViewGrid'
import { InsightsIcon } from './icon'
import PlusIcon from 'mdi-react/PlusIcon'
import { Link } from '../../../shared/src/components/Link'
import GearIcon from 'mdi-react/GearIcon'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { PageHeader } from '../components/PageHeader'
import { BreadcrumbsProps, BreadcrumbSetters } from '../components/Breadcrumbs'
import { StatusBadge } from '../components/StatusBadge'
import { Page } from '../components/Page'
import { TelemetryProps } from '../../../shared/src/telemetry/telemetryService'

interface InsightsPageProps
    extends ExtensionsControllerProps,
        Omit<ViewGridProps, 'views'>,
        BreadcrumbsProps,
        BreadcrumbSetters,
        TelemetryProps {}
export const InsightsPage: React.FunctionComponent<InsightsPageProps> = props => {
    props.useBreadcrumb(
        useMemo(
            () => ({
                key: 'Insights',
                element: <>Insights</>,
            }),
            []
        )
    )

    const views = useObservable(
        useMemo(
            () =>
                getViewsForContainer(
                    ContributableViewContainer.InsightsPage,
                    {},
                    props.extensionsController.services.view
                ),
            [props.extensionsController.services.view]
        )
    )

    useEffect(() => {
        props.telemetryService.logViewEvent('Insights')
    }, [props.telemetryService])

    const logConfigureClick = useCallback(() => {
        props.telemetryService.log('InsightConfigureClick')
    }, [props.telemetryService])

    const logAddMoreClick = useCallback(() => {
        props.telemetryService.log('InsightAddMoreClick')
    }, [props.telemetryService])

    return (
        <div className="w-100">
            <Page>
                <PageHeader
                    annotation={<StatusBadge status="prototype" feedback={{ mailto: 'support@sourcegraph.com' }} />}
                    path={[{ icon: InsightsIcon, text: 'Code insights' }]}
                    actions={
                        <>
                            <Link
                                to="/extensions?query=category:Insights"
                                onClick={logAddMoreClick}
                                className="btn btn-secondary mr-1"
                            >
                                <PlusIcon className="icon-inline" /> Add more insights
                            </Link>
                            <Link to="/user/settings" onClick={logConfigureClick} className="btn btn-secondary">
                                <GearIcon className="icon-inline" /> Configure insights
                            </Link>
                        </>
                    }
                    className="mb-3"
                />
                {views === undefined ? (
                    <div className="d-flex w-100">
                        <LoadingSpinner className="my-4" />
                    </div>
                ) : (
                    <ViewGrid {...props} views={views} />
                )}
            </Page>
        </div>
    )
}
