import { FunctionComponent, useCallback, useState } from 'react'

import { Subject } from 'rxjs'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { Button, Alert, Input, Label } from '@sourcegraph/wildcard'

import { useEnqueueIndexJob } from '../hooks/useEnqueueIndexJob'

export interface EnqueueFormProps {
    repoId: string
    querySubject: Subject<string>
}

enum State {
    Idle,
    Queueing,
    Queued,
}

export const EnqueueForm: FunctionComponent<React.PropsWithChildren<EnqueueFormProps>> = ({ repoId, querySubject }) => {
    const [revlike, setRevlike] = useState('HEAD')
    const [state, setState] = useState(() => State.Idle)
    const [queueResult, setQueueResult] = useState<number>()
    const [enqueueError, setEnqueueError] = useState<Error>()
    const { handleEnqueueIndexJob } = useEnqueueIndexJob()

    const enqueue = useCallback(async () => {
        setState(State.Queueing)
        setEnqueueError(undefined)
        setQueueResult(undefined)

        try {
            const indexes = await handleEnqueueIndexJob({
                variables: { id: repoId, rev: revlike },
            }).then(({ data }) => data)

            const queueResultLength = indexes?.queueAutoIndexJobsForRepo.length || 0
            setQueueResult(queueResultLength)
            if (queueResultLength > 0) {
                querySubject.next(indexes?.queueAutoIndexJobsForRepo[0].inputCommit)
            }
        } catch (error) {
            setEnqueueError(error)
            setQueueResult(undefined)
        } finally {
            setState(State.Queued)
        }
    }, [repoId, revlike, querySubject, handleEnqueueIndexJob])

    return (
        <>
            {enqueueError && <ErrorAlert prefix="Error enqueueing index job" error={enqueueError} />}

            <div className="form-inline">
                <Label htmlFor="revlike">Git revlike</Label>

                <Input
                    id="revlike"
                    className="ml-2"
                    value={revlike}
                    onChange={event => setRevlike(event.target.value)}
                />

                <Button
                    type="button"
                    title="Enqueue thing"
                    disabled={state === State.Queueing}
                    className="ml-2"
                    variant="primary"
                    onClick={enqueue}
                >
                    Enqueue
                </Button>
            </div>

            {state === State.Queued &&
                queueResult !== undefined &&
                (queueResult > 0 ? (
                    <Alert className="mt-3 mb-0" variant="success">
                        {queueResult} auto-indexing jobs enqueued.
                    </Alert>
                ) : (
                    <Alert className="mt-3 mb-0" variant="info">
                        Failed to enqueue any auto-indexing jobs.
                        <br />
                        Check if the auto-index configuration is up-to-date.
                    </Alert>
                ))}
        </>
    )
}
