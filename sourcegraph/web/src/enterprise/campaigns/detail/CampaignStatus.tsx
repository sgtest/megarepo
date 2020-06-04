import React, { useState } from 'react'
import * as GQL from '../../../../../shared/src/graphql/schema'
import { ErrorMessage } from '../../../components/alerts'
import SyncIcon from 'mdi-react/SyncIcon'
import { pluralize } from '../../../../../shared/src/util/strings'
import { retryCampaignChangesets } from './backend'
import { asError, isErrorLike } from '../../../../../shared/src/util/errors'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import ErrorIcon from 'mdi-react/ErrorIcon'
import * as H from 'history'

export interface CampaignStatusProps {
    campaign: Pick<GQL.ICampaign, 'id' | 'closedAt' | 'viewerCanAdminister'> & {
        changesets: Pick<GQL.ICampaign['changesets'], 'totalCount'>
        status: Pick<GQL.ICampaign['status'], 'completedCount' | 'pendingCount' | 'errors' | 'state'>
    }

    /** Called when the "Retry failed jobs" button is clicked. */
    afterRetry: (updatedCampaign: GQL.ICampaign) => void
    history: H.History
}

type CampaignState = 'closed' | 'errored' | 'processing' | 'completed'

/**
 * The status of a campaign's jobs, plus its closed state and errors.
 */
export const CampaignStatus: React.FunctionComponent<CampaignStatusProps> = ({ campaign, afterRetry, history }) => {
    const { status } = campaign

    const progress = (status.completedCount / (status.pendingCount + status.completedCount)) * 100

    let state: CampaignState
    if (campaign.closedAt) {
        state = 'closed'
    } else if (campaign.status.state === GQL.BackgroundProcessState.ERRORED) {
        state = 'errored'
    } else if (campaign.status.state === GQL.BackgroundProcessState.PROCESSING) {
        state = 'processing'
    } else {
        state = 'completed'
    }

    const errorList = (
        <ul className="mt-2">
            {status.errors.map((error, index) => (
                <li className="mb-2" key={index}>
                    <p className="mb-0">
                        <ErrorMessage error={error} history={history} />
                    </p>
                </li>
            ))}
        </ul>
    )

    const [isRetrying, setIsRetrying] = useState<boolean | Error>(false)

    const onRetry: React.MouseEventHandler = async (): Promise<void> => {
        setIsRetrying(true)
        try {
            const retriedCampaign = await retryCampaignChangesets(campaign.id)
            setIsRetrying(false)
            afterRetry(retriedCampaign)
        } catch (error) {
            setIsRetrying(asError(error))
        }
    }

    let statusIndicator: JSX.Element | undefined
    switch (state) {
        case 'errored':
            statusIndicator = (
                <>
                    <div className="alert alert-danger my-4">
                        <h3 className="alert-heading mb-0">Creating changesets failed</h3>
                        {errorList}
                        {campaign.viewerCanAdminister && (
                            <button
                                type="button"
                                className="btn btn-primary mb-0"
                                onClick={onRetry}
                                disabled={isRetrying === true}
                            >
                                {isErrorLike(isRetrying) && (
                                    <ErrorIcon data-tooltip={isRetrying.message} className="mr-2" />
                                )}
                                {isRetrying === true && <LoadingSpinner className="icon-inline" />}
                                Retry
                            </button>
                        )}
                        {!campaign.viewerCanAdminister && (
                            <p className="mb-0">You don't have permission to view error details.</p>
                        )}
                    </div>
                </>
            )
            break
        case 'processing':
            statusIndicator = (
                <>
                    <div className="alert alert-info mt-4">
                        <p>
                            <SyncIcon className="icon-inline" /> Creating {status.pendingCount}{' '}
                            {pluralize('changeset', status.pendingCount)} on code hosts...
                        </p>
                        <div className="progress mt-2 mb-1">
                            {/* we need to set the width to control the progress bar, so: */}
                            {/* eslint-disable-next-line react/forbid-dom-props */}
                            <div className="progress-bar" style={{ width: progress + '%' }}>
                                &nbsp;
                            </div>
                        </div>
                        {status.errors.length > 0 && <h4 className="mt-1">Creating changesets failed</h4>}
                        {errorList}
                    </div>
                </>
            )
            break
        case 'closed':
            statusIndicator = (
                <div className="alert alert-secondary mt-2">
                    Campaign is closed. No changes can be made to this campaign anymore.
                </div>
            )
            break
    }

    if (!statusIndicator) {
        return null
    }
    return <div>{statusIndicator}</div>
}
