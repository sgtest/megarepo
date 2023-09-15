import * as React from 'react'

import { mdiClipboardPulseOutline } from '@mdi/js'
import classNames from 'classnames'

import type { Progress, StreamingResultsState } from '@sourcegraph/shared/src/search/stream'
import { Icon, Link } from '@sourcegraph/wildcard'

import { StreamingProgressCount } from './StreamingProgressCount'
import { StreamingProgressSkippedButton } from './StreamingProgressSkippedButton'

import styles from './StreamingProgressCount.module.scss'

export interface StreamingProgressProps {
    query: string
    state: StreamingResultsState
    progress: Progress
    showTrace?: boolean
    onSearchAgain: (additionalFilters: string[]) => void
    isSearchJobsEnabled?: boolean
}

export const StreamingProgress: React.FunctionComponent<React.PropsWithChildren<StreamingProgressProps>> = ({
    progress,
    query,
    state,
    showTrace,
    onSearchAgain,
    isSearchJobsEnabled,
}) => {
    const isLoading = state === 'loading'

    return (
        <>
            {isLoading && <StreamingProgressCount progress={progress} state={state} hideIcon={true} />}
            {!isLoading && (
                <StreamingProgressSkippedButton
                    query={query}
                    progress={progress}
                    isSearchJobsEnabled={isSearchJobsEnabled}
                    onSearchAgain={onSearchAgain}
                />
            )}
            <TraceLink showTrace={showTrace} trace={progress.trace} />
        </>
    )
}

const TraceLink: React.FunctionComponent<{ showTrace?: boolean; trace?: string }> = ({ showTrace, trace }) =>
    showTrace && trace ? (
        <small className={classNames('d-flex align-items-center', styles.count)}>
            <Link to={trace}>
                <Icon aria-hidden={true} className="mr-2" svgPath={mdiClipboardPulseOutline} />
                View trace
            </Link>
        </small>
    ) : (
        <></>
    )
