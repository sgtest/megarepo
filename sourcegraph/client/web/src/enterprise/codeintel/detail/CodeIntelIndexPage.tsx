import { useApolloClient } from '@apollo/client'
import React, { FunctionComponent, useCallback, useEffect, useMemo, useState } from 'react'
import { Redirect, RouteComponentProps } from 'react-router'
import { takeWhile } from 'rxjs/operators'

import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { LSIFIndexState } from '@sourcegraph/shared/src/graphql-operations'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ErrorLike, isErrorLike } from '@sourcegraph/shared/src/util/errors'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'
import { Container, PageHeader } from '@sourcegraph/wildcard'

import { ErrorAlert } from '../../../components/alerts'
import { PageTitle } from '../../../components/PageTitle'
import { LsifIndexFields } from '../../../graphql-operations'
import { CodeIntelStateBanner } from '../shared/CodeIntelStateBanner'

import { CodeIntelAssociatedUpload } from './CodeIntelAssociatedUpload'
import { CodeIntelDeleteIndex } from './CodeIntelDeleteIndex'
import { CodeIntelIndexMeta } from './CodeIntelIndexMeta'
import { CodeIntelIndexTimeline } from './CodeIntelIndexTimeline'
import { queryLisfIndex as defaultQueryLsifIndex, useDeleteLsifIndex } from './useLsifIndex'

export interface CodeIntelIndexPageProps extends RouteComponentProps<{ id: string }>, TelemetryProps {
    queryLisfIndex?: typeof defaultQueryLsifIndex
    now?: () => Date
}

const classNamesByState = new Map([
    [LSIFIndexState.COMPLETED, 'alert-success'],
    [LSIFIndexState.ERRORED, 'alert-danger'],
])

export const CodeIntelIndexPage: FunctionComponent<CodeIntelIndexPageProps> = ({
    match: {
        params: { id },
    },
    queryLisfIndex = defaultQueryLsifIndex,
    telemetryService,
    now,
    history,
}) => {
    useEffect(() => telemetryService.logViewEvent('CodeIntelIndex'), [telemetryService])

    const apolloClient = useApolloClient()
    const [deletionOrError, setDeletionOrError] = useState<'loading' | 'deleted' | ErrorLike>()
    const { handleDeleteLsifIndex, deleteError } = useDeleteLsifIndex()

    useEffect(() => {
        if (deleteError) {
            setDeletionOrError(deleteError)
        }
    }, [deleteError])

    const indexOrError = useObservable(
        useMemo(() => queryLisfIndex(id, apolloClient).pipe(takeWhile(shouldReload, true)), [
            id,
            queryLisfIndex,
            apolloClient,
        ])
    )

    const deleteIndex = useCallback(async (): Promise<void> => {
        if (!indexOrError || isErrorLike(indexOrError)) {
            return
        }

        const autoIndexCommit = indexOrError.inputCommit.slice(0, 7)
        if (!window.confirm(`Delete auto-index record for commit ${autoIndexCommit}?`)) {
            return
        }

        setDeletionOrError('loading')

        try {
            await handleDeleteLsifIndex({
                variables: { id },
                update: cache => cache.modify({ fields: { node: () => {} } }),
            })
            setDeletionOrError('deleted')
            history.push({
                state: {
                    modal: 'SUCCESS',
                    message: `Auto-index record for commit ${autoIndexCommit} has been deleted.`,
                },
            })
        } catch (error) {
            setDeletionOrError(error)
            history.push({
                state: {
                    modal: 'ERROR',
                    message: `There was an error while saving auto-index record for commit: ${autoIndexCommit}.`,
                },
            })
        }
    }, [id, indexOrError, handleDeleteLsifIndex, history])

    return deletionOrError === 'deleted' ? (
        <Redirect to="." />
    ) : isErrorLike(deletionOrError) ? (
        <ErrorAlert prefix="Error deleting LSIF index record" error={deletionOrError} />
    ) : (
        <div className="site-admin-lsif-index-page w-100">
            <PageTitle title="Auto-indexing jobs" />
            {isErrorLike(indexOrError) ? (
                <ErrorAlert prefix="Error loading LSIF index" error={indexOrError} />
            ) : !indexOrError ? (
                <LoadingSpinner className="icon-inline" />
            ) : (
                <>
                    <PageHeader
                        headingElement="h2"
                        path={[
                            {
                                text: `Auto-index record for ${indexOrError.projectRoot?.repository.name || ''}@${
                                    indexOrError.projectRoot
                                        ? indexOrError.projectRoot.commit.abbreviatedOID
                                        : indexOrError.inputCommit.slice(0, 7)
                                }`,
                            },
                        ]}
                        className="mb-3"
                    />

                    <Container>
                        <CodeIntelIndexMeta node={indexOrError} now={now} />
                    </Container>

                    <Container className="mt-2">
                        <CodeIntelStateBanner
                            state={indexOrError.state}
                            placeInQueue={indexOrError.placeInQueue}
                            failure={indexOrError.failure}
                            typeName="index"
                            pluralTypeName="indexes"
                            className={classNamesByState.get(indexOrError.state)}
                        />
                    </Container>

                    <Container className="mt-2">
                        <CodeIntelDeleteIndex deleteIndex={deleteIndex} deletionOrError={deletionOrError} />
                    </Container>

                    <Container className="mt-2">
                        <h3>Timeline</h3>
                        <CodeIntelIndexTimeline index={indexOrError} now={now} className="mb-3" />
                        <CodeIntelAssociatedUpload node={indexOrError} now={now} />
                    </Container>
                </>
            )}
        </div>
    )
}

const terminalStates = new Set([LSIFIndexState.COMPLETED, LSIFIndexState.ERRORED])

function shouldReload(index: LsifIndexFields | ErrorLike | null | undefined): boolean {
    return !isErrorLike(index) && !(index && terminalStates.has(index.state))
}
