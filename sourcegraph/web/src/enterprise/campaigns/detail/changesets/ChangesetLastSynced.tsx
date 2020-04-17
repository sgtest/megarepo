import React, { useState, useEffect } from 'react'
import { IExternalChangeset } from '../../../../../../shared/src/graphql/schema'
import classNames from 'classnames'
import { formatDistance, parseISO } from 'date-fns'
import { syncChangeset } from '../backend'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import SyncIcon from 'mdi-react/SyncIcon'
import { Observer } from 'rxjs'
import ErrorIcon from 'mdi-react/ErrorIcon'
import { isErrorLike } from '../../../../../../shared/src/util/errors'

interface Props {
    changeset: Pick<IExternalChangeset, 'id' | 'nextSyncAt' | 'updatedAt'>
    campaignUpdates?: Pick<Observer<void>, 'next'>

    /** For testing purposes only */
    _now?: Date
}

export const ChangesetLastSynced: React.FunctionComponent<Props> = ({ changeset, campaignUpdates, _now }) => {
    // initially, the changeset was never last updated
    const [lastUpdatedAt, setLastUpdatedAt] = useState<string | Error | null>(null)
    // .. if it was, and the changesets current updatedAt doesn't match the previous updated at, we know that it has been synced
    const lastUpdatedAtChanged = lastUpdatedAt && !isErrorLike(lastUpdatedAt) && changeset.updatedAt !== lastUpdatedAt
    useEffect(() => {
        if (lastUpdatedAtChanged) {
            if (campaignUpdates) {
                campaignUpdates.next()
            }
            setLastUpdatedAt(null)
        }
    }, [campaignUpdates, lastUpdatedAtChanged, changeset.updatedAt])
    const enqueueChangeset: React.MouseEventHandler = async () => {
        // already enqueued
        if (typeof lastUpdatedAt === 'string') {
            return
        }
        setLastUpdatedAt(changeset.updatedAt)
        try {
            await syncChangeset(changeset.id)
        } catch (error) {
            setLastUpdatedAt(error)
        }
    }

    let tooltipText = ''
    if (changeset.updatedAt === lastUpdatedAt) {
        tooltipText = 'Currently refreshing'
    } else {
        if (!changeset.nextSyncAt) {
            tooltipText = 'Not scheduled for syncing.'
        } else {
            tooltipText = `Next refresh in ${formatDistance(parseISO(changeset.nextSyncAt), _now ?? new Date())}.`
        }
        tooltipText += ' Click to prioritize refresh'
    }

    const UpdateLoaderIcon =
        typeof lastUpdatedAt === 'string' && changeset.updatedAt === lastUpdatedAt ? LoadingSpinner : SyncIcon

    return (
        <small className="text-muted ml-2">
            Last synced {formatDistance(parseISO(changeset.updatedAt), _now ?? new Date())} ago.{' '}
            {isErrorLike(lastUpdatedAt) && (
                <ErrorIcon data-tooltip={lastUpdatedAt.message} className="ml-2 icon-inline small" />
            )}
            <span data-tooltip={tooltipText}>
                <UpdateLoaderIcon
                    className={classNames('icon-inline', typeof lastUpdatedAt !== 'string' && 'cursor-pointer')}
                    onClick={enqueueChangeset}
                />
            </span>
        </small>
    )
}
