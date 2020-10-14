import React from 'react'

export interface CampaignChangesetsHeaderProps {
    // Nothing.
}

export const CampaignChangesetsHeader: React.FunctionComponent<CampaignChangesetsHeaderProps> = () => (
    <>
        <span className="d-none d-md-block" />
        <h5 className="d-none d-md-block text-uppercase text-center text-nowrap">Status</h5>
        <h5 className="d-none d-md-block text-uppercase text-nowrap">Changeset information</h5>
        <h5 className="d-none d-md-block text-uppercase text-center text-nowrap">Check state</h5>
        <h5 className="d-none d-md-block text-uppercase text-center text-nowrap">Review state</h5>
        <h5 className="d-none d-md-block text-uppercase text-center text-nowrap">Changes</h5>
    </>
)
