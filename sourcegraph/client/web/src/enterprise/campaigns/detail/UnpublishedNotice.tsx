import React from 'react'
import classNames from 'classnames'
import { pluralize } from '../../../../../shared/src/util/strings'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'

interface UnpublishedNoticeProps {
    unpublished: number
    className?: string
}

export const UnpublishedNotice: React.FunctionComponent<UnpublishedNoticeProps> = ({ unpublished, className }) => {
    if (unpublished === 0) {
        return <></>
    }
    return (
        <div className={classNames('alert alert-secondary', className)}>
            <SourceBranchIcon className="icon-inline" /> {unpublished} unpublished{' '}
            {pluralize('changeset', unpublished, 'changesets')}. Set{' '}
            <code className="badge badge-secondary">published: true</code> in the campaign spec and re-run{' '}
            <code className="badge badge-secondary">src campaign apply -f $CAMPAIGN_SPEC_FILE</code> to publish to your
            code host.
        </div>
    )
}
