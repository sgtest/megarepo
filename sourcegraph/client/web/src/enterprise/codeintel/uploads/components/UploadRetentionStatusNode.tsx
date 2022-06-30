import { FunctionComponent } from 'react'

import { mdiInformationOutline } from '@mdi/js'
import classNames from 'classnames'

import { pluralize } from '@sourcegraph/common'
import { Link, Icon, H3 } from '@sourcegraph/wildcard'

import {
    NormalizedUploadRetentionMatch,
    RetentionPolicyMatch,
    UploadReferenceMatch,
} from '../hooks/queryUploadRetentionMatches'

import styles from './DependencyOrDependentNode.module.scss'

export interface RetentionMatchNodeProps {
    node: NormalizedUploadRetentionMatch
}

export const retentionByUploadTitle = 'Retention by reference'
export const retentionByBranchTipTitle = 'Retention by tip of default branch'

export const RetentionMatchNode: FunctionComponent<React.PropsWithChildren<RetentionMatchNodeProps>> = ({ node }) => {
    if (node.matchType === 'RetentionPolicy') {
        return <RetentionPolicyRetentionMatchNode match={node} />
    }
    if (node.matchType === 'UploadReference') {
        return <UploadReferenceRetentionMatchNode match={node} />
    }

    throw new Error(`invalid node type ${JSON.stringify(node as object)}`)
}

const RetentionPolicyRetentionMatchNode: FunctionComponent<
    React.PropsWithChildren<{ match: RetentionPolicyMatch }>
> = ({ match }) => (
    <>
        <span className={styles.separator} />

        <div className={classNames(styles.information, 'd-flex flex-column')}>
            <div className="m-0">
                {match.configurationPolicy ? (
                    <Link to={`../configuration/${match.configurationPolicy.id}`} className="p-0">
                        <H3 className="m-0 d-block d-md-inline">{match.configurationPolicy.name}</H3>
                    </Link>
                ) : (
                    <H3 className="m-0 d-block d-md-inline">{retentionByBranchTipTitle}</H3>
                )}
                <div className="mr-2 d-block d-mdinline-block">
                    Retained: {match.matches ? 'yes' : 'no'}
                    {match.protectingCommits.length !== 0 && (
                        <>
                            , by {match.protectingCommits.length} visible{' '}
                            {pluralize('commit', match.protectingCommits.length)}, including{' '}
                            {match.protectingCommits
                                .slice(0, 4)
                                .map(hash => hash.slice(0, 7))
                                .join(', ')}
                            <Icon
                                className="ml-1"
                                aria-label="This upload is retained to service code-intel queries for commit(s) with applicable retention policies."
                                data-tooltip="This upload is retained to service code-intel queries for commit(s) with applicable retention policies."
                                svgPath={mdiInformationOutline}
                            />
                        </>
                    )}
                    {!match.configurationPolicy && (
                        <Icon
                            className="ml-1"
                            aria-label="Uploads at the tip of the default branch are always retained indefinitely."
                            data-tooltip="Uploads at the tip of the default branch are always retained indefinitely."
                            svgPath={mdiInformationOutline}
                        />
                    )}
                </div>
            </div>
        </div>
    </>
)

const UploadReferenceRetentionMatchNode: FunctionComponent<
    React.PropsWithChildren<{ match: UploadReferenceMatch }>
> = ({ match }) => (
    <>
        <span className={styles.separator} />

        <div className={classNames(styles.information, 'd-flex flex-column')}>
            <div className="m-0">
                <H3 className="m-0 d-block d-md-inline">{retentionByUploadTitle}</H3>
                <div className="mr-2 d-block d-mdinline-block">
                    Referenced by {match.total} {pluralize('upload', match.total, 'uploads')}, including{' '}
                    {match.uploadSlice
                        .slice(0, 3)
                        .map<React.ReactNode>(upload => (
                            <Link key={upload.id} to={`/site-admin/code-intelligence/uploads/${upload.id}`}>
                                {upload.projectRoot?.repository.name ?? 'unknown'}
                            </Link>
                        ))
                        .reduce((previous, current) => [previous, ', ', current])}
                    <Icon
                        className="ml-1"
                        aria-label="Uploads that are dependencies of other upload(s) are retained to service cross-repository code-intel queries."
                        data-tooltip="Uploads that are dependencies of other upload(s) are retained to service cross-repository code-intel queries."
                        svgPath={mdiInformationOutline}
                    />
                </div>
            </div>
        </div>
    </>
)
