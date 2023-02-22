import { FC, useCallback, useEffect, useMemo } from 'react'

import { noop } from 'lodash'
import { useParams } from 'react-router-dom'
import { NEVER } from 'rxjs'
import { catchError, startWith } from 'rxjs/operators'

import { asError, isErrorLike } from '@sourcegraph/common'
import {
    FetchFileParameters,
    fetchHighlightedFileLineRanges as fetchHighlightedFileLineRangesShared,
} from '@sourcegraph/shared/src/backend/file'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { aggregateStreamingSearch } from '@sourcegraph/shared/src/search/stream'
import { Alert, LoadingSpinner, useObservable } from '@sourcegraph/wildcard'

import { eventLogger } from '../../tracking/eventLogger'
import { fetchNotebook } from '../backend'
import { convertNotebookTitleToFileName } from '../serialize'

import { NotebookContent, NotebookContentProps } from './NotebookContent'

interface EmbeddedNotebookPageProps
    extends Pick<
            NotebookContentProps,
            'searchContextsEnabled' | 'isSourcegraphDotCom' | 'authenticatedUser' | 'settingsCascade'
        >,
        PlatformContextProps<'sourcegraphURL' | 'requestGraphQL' | 'urlToFile' | 'settings'> {}

const LOADING = 'loading' as const

export const EmbeddedNotebookPage: FC<EmbeddedNotebookPageProps> = ({ platformContext, ...props }) => {
    const { notebookId } = useParams()

    useEffect(() => eventLogger.logPageView('EmbeddedNotebookPage'), [])

    const notebookOrError = useObservable(
        useMemo(
            () =>
                fetchNotebook(notebookId!).pipe(
                    startWith(LOADING),
                    catchError(error => [asError(error)])
                ),
            [notebookId]
        )
    )

    const fetchHighlightedFileLineRanges = useCallback(
        (parameters: FetchFileParameters, force?: boolean) =>
            fetchHighlightedFileLineRangesShared(
                {
                    ...parameters,
                    platformContext,
                },
                force
            ),
        [platformContext]
    )

    return (
        <div className="p-3">
            {notebookOrError === LOADING && (
                <div className="d-flex justify-content-center">
                    <LoadingSpinner />
                </div>
            )}
            {isErrorLike(notebookOrError) && (
                <Alert variant="danger">
                    Error while loading the notebook: <strong>{notebookOrError.message}</strong>
                </Alert>
            )}
            {notebookOrError && notebookOrError !== LOADING && !isErrorLike(notebookOrError) && (
                <NotebookContent
                    {...props}
                    blocks={notebookOrError.blocks}
                    onUpdateBlocks={noop}
                    viewerCanManage={false}
                    globbing={true}
                    fetchHighlightedFileLineRanges={fetchHighlightedFileLineRanges}
                    streamSearch={aggregateStreamingSearch}
                    telemetryService={eventLogger}
                    platformContext={platformContext}
                    exportedFileName={convertNotebookTitleToFileName(notebookOrError.title)}
                    // Copying is not supported in embedded notebooks
                    onCopyNotebook={() => NEVER}
                    isEmbedded={true}
                />
            )}
        </div>
    )
}
