import React from 'react'
import { DismissibleAlert } from '../../../components/DismissibleAlert'

export interface CampaignsBetaFeedbackAlertProps {
    /** The key where to store the flag whether the alert was dismissed. */
    partialStorageKey?: string
}

export const CampaignsBetaFeedbackAlert: React.FunctionComponent<{ partialStorageKey?: string }> = ({
    partialStorageKey = 'campaigns-beta',
}) => (
    <DismissibleAlert partialStorageKey={partialStorageKey} className="alert-info">
        <p className="mb-0">
            Campaigns are currently in beta. During the beta period, campaigns are free to use. After the beta period,
            campaigns will be available as a paid add-on. Get in touch on Twitter{' '}
            <a href="https://twitter.com/srcgraph">@srcgraph</a>, file an issue in our{' '}
            <a href="https://github.com/sourcegraph/sourcegraph/issues">public issue tracker</a>, or email{' '}
            <a href="mailto:feedback@sourcegraph.com?subject=Feedback on Campaigns">feedback@sourcegraph.com</a>. We're
            looking forward to your feedback!
        </p>
    </DismissibleAlert>
)
