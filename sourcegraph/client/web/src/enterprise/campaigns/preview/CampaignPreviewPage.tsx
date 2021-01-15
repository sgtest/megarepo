import * as H from 'history'
import React, { useEffect, useMemo } from 'react'
import { useObservable } from '../../../../../shared/src/util/useObservable'
import { delay, distinctUntilChanged, repeatWhen } from 'rxjs/operators'
import { isEqual } from 'lodash'
import { PageTitle } from '../../../components/PageTitle'
import { fetchCampaignSpecById as _fetchCampaignSpecById } from './backend'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { CampaignHeader } from '../detail/CampaignHeader'
import { PreviewList } from './list/PreviewList'
import { ThemeProps } from '../../../../../shared/src/theme'
import { CreateUpdateCampaignAlert } from './CreateUpdateCampaignAlert'
import AlertCircleIcon from 'mdi-react/AlertCircleIcon'
import { HeroPage } from '../../../components/HeroPage'
import { CampaignDescription } from '../detail/CampaignDescription'
import { CampaignSpecInfoByline } from './CampaignSpecInfoByline'
import { TelemetryProps } from '../../../../../shared/src/telemetry/telemetryService'
import { AuthenticatedUser } from '../../../auth'
import { MissingCredentialsAlert } from './MissingCredentialsAlert'
import { SupersedingCampaignSpecAlert } from '../detail/SupersedingCampaignSpecAlert'
import { queryChangesetSpecFileDiffs, queryChangesetApplyPreview } from './list/backend'
import { CampaignPreviewStatsBar } from './CampaignPreviewStatsBar'

export interface CampaignPreviewPageProps extends ThemeProps, TelemetryProps {
    campaignSpecID: string
    history: H.History
    location: H.Location
    authenticatedUser: Pick<AuthenticatedUser, 'url'>

    /** Used for testing. */
    fetchCampaignSpecById?: typeof _fetchCampaignSpecById
    /** Used for testing. */
    queryChangesetApplyPreview?: typeof queryChangesetApplyPreview
    /** Used for testing. */
    queryChangesetSpecFileDiffs?: typeof queryChangesetSpecFileDiffs
    /** Expand changeset descriptions, for testing only. */
    expandChangesetDescriptions?: boolean
}

export const CampaignPreviewPage: React.FunctionComponent<CampaignPreviewPageProps> = ({
    campaignSpecID: specID,
    history,
    location,
    authenticatedUser,
    isLightTheme,
    telemetryService,
    fetchCampaignSpecById = _fetchCampaignSpecById,
    queryChangesetApplyPreview,
    queryChangesetSpecFileDiffs,
    expandChangesetDescriptions,
}) => {
    const spec = useObservable(
        useMemo(
            () =>
                fetchCampaignSpecById(specID).pipe(
                    repeatWhen(notifier => notifier.pipe(delay(5000))),
                    distinctUntilChanged(isEqual)
                ),
            [specID, fetchCampaignSpecById]
        )
    )

    useEffect(() => {
        telemetryService.logViewEvent('CampaignApplyPage')
    }, [telemetryService])

    if (spec === undefined) {
        return (
            <div className="text-center">
                <LoadingSpinner className="icon-inline mx-auto my-4" />
            </div>
        )
    }
    if (spec === null) {
        return <HeroPage icon={AlertCircleIcon} title="Campaign spec not found" />
    }

    return (
        <>
            <PageTitle title="Apply campaign spec" />
            <CampaignHeader
                name={spec.description.name}
                namespace={spec.namespace}
                className="test-campaign-apply-page"
            />
            <CampaignSpecInfoByline createdAt={spec.createdAt} creator={spec.creator} className="mb-3" />
            <MissingCredentialsAlert
                authenticatedUser={authenticatedUser}
                viewerCampaignsCodeHosts={spec.viewerCampaignsCodeHosts}
            />
            <SupersedingCampaignSpecAlert spec={spec.supersedingCampaignSpec} />
            <CampaignPreviewStatsBar campaignSpec={spec} />
            <CreateUpdateCampaignAlert
                history={history}
                specID={spec.id}
                campaign={spec.appliesToCampaign}
                viewerCanAdminister={spec.viewerCanAdminister}
                telemetryService={telemetryService}
            />
            <CampaignDescription history={history} description={spec.description.description} />
            <PreviewList
                campaignSpecID={specID}
                history={history}
                location={location}
                isLightTheme={isLightTheme}
                queryChangesetApplyPreview={queryChangesetApplyPreview}
                queryChangesetSpecFileDiffs={queryChangesetSpecFileDiffs}
                expandChangesetDescriptions={expandChangesetDescriptions}
            />
        </>
    )
}
