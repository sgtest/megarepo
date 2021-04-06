import React, { useCallback, useEffect, useMemo } from 'react'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { ViewGrid, ViewGridProps } from '../repo/tree/ViewGrid'
import { InsightsIcon } from './icon'
import PlusIcon from 'mdi-react/PlusIcon'
import { Link } from '@sourcegraph/shared/src/components/Link'
import GearIcon from 'mdi-react/GearIcon'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { PageHeader } from '../components/PageHeader'
import { FeedbackBadge } from '../components/FeedbackBadge'
import { Page } from '../components/Page'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { getCombinedViews } from './backend'
import { from } from 'rxjs'
import { switchMap } from 'rxjs/operators'
import { wrapRemoteObservable } from '@sourcegraph/shared/src/api/client/api/common'

interface InsightsPageProps extends ExtensionsControllerProps, Omit<ViewGridProps, 'views'>, TelemetryProps {}

export const InsightsPage: React.FunctionComponent<InsightsPageProps> = props => {
    const views = useObservable(
        useMemo(
            () =>
                getCombinedViews(() =>
                    from(props.extensionsController.extHostAPI).pipe(
                        switchMap(extensionHostAPI => wrapRemoteObservable(extensionHostAPI.getInsightsViews({})))
                    )
                ),
            [props.extensionsController]
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
                    annotation={<FeedbackBadge status="prototype" feedback={{ mailto: 'support@sourcegraph.com' }} />}
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
