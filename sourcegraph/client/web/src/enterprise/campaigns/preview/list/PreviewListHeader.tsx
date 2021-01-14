import React from 'react'

export interface PreviewListHeaderProps {
    // Nothing for now.
}

export const PreviewListHeader: React.FunctionComponent<PreviewListHeaderProps> = () => (
    <>
        <span className="p-3 d-none d-sm-block" />
        <h5 className="p-3 d-none d-sm-block text-uppercase text-center">Current state</h5>
        <h5 className="p-3 d-none d-sm-block text-uppercase text-center text-nowrap">Action</h5>
        <h5 className="p-3 d-none d-sm-block text-uppercase text-nowrap">Changeset information</h5>
        <h5 className="p-3 d-none d-sm-block text-uppercase text-center text-nowrap">Changes</h5>
    </>
)
