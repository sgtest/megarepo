import React, { useState, useEffect, Dispatch, SetStateAction } from 'react'

import { ApolloError, useQuery } from '@apollo/client'
import * as H from 'history'
import { useHistory } from 'react-router-dom'

import { gql, getDocumentNode } from '@sourcegraph/http-client'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'

import { FuzzySearch, SearchIndexing } from '../../fuzzyFinder/FuzzySearch'
import { FileNamesResult, FileNamesVariables } from '../../graphql-operations'
import { parseBrowserRepoURL } from '../../util/url'

import { FuzzyModal } from './FuzzyModal'

const DEFAULT_MAX_RESULTS = 100

export interface FuzzyFinderProps extends TelemetryProps {
    setIsVisible: Dispatch<SetStateAction<boolean>>

    isVisible: boolean

    location: H.Location

    setCacheRetention: Dispatch<SetStateAction<boolean>>

    /**
     * The maximum number of files a repo can have to use case-insensitive fuzzy finding.
     *
     * Case-insensitive fuzzy finding is more expensive to compute compared to
     * word-sensitive fuzzy finding.  The fuzzy modal will use case-insensitive
     * fuzzy finding when the repo has fewer files than this number, and
     * word-sensitive fuzzy finding otherwise.
     */
    caseInsensitiveFileCountThreshold?: number
}

export const FuzzyFinder: React.FunctionComponent<React.PropsWithChildren<FuzzyFinderProps>> = ({
    location: { search, pathname, hash },
    setCacheRetention,
    setIsVisible,
    isVisible,
    telemetryService,
}) => {
    // The state machine of the fuzzy finder. See `FuzzyFSM` for more details
    // about the state transititions.
    const [fsm, setFsm] = useState<FuzzyFSM>({ key: 'empty' })
    const { repoName = '', commitID = '', rawRevision = '' } = parseBrowserRepoURL(pathname + search + hash)
    const { downloadFilename, isLoadingFilename, filenameError } = useFilename(repoName, commitID || rawRevision)

    const history = useHistory()
    useEffect(
        () =>
            history.listen(location => {
                const url = location.pathname + location.search + location.hash
                const { repoName: repo = '', commitID: commit = '', rawRevision: raw = '' } = parseBrowserRepoURL(url)
                if (repo !== repoName || commit !== commitID || raw !== rawRevision) {
                    setCacheRetention(false)
                }
            }),
        [history, repoName, commitID, rawRevision, setCacheRetention]
    )

    useEffect(() => {
        if (isVisible) {
            telemetryService.log('FuzzyFinderViewed', { action: 'shortcut open' })
        }
    }, [telemetryService, isVisible])

    if (!isVisible) {
        return null
    }

    return (
        <FuzzyModal
            repoName={repoName}
            commitID={commitID}
            initialMaxResults={DEFAULT_MAX_RESULTS}
            initialQuery=""
            downloadFilenames={downloadFilename}
            isLoading={isLoadingFilename}
            isError={filenameError}
            onClose={() => setIsVisible(false)}
            fsm={fsm}
            setFsm={setFsm}
        />
    )
}

/**
 * The fuzzy finder modal is implemented as a state machine with the following transitions:
 *
 * ```
 *   ╭────[cached]───────────────────────╮  ╭──╮
 *   │                                   v  │  v
 * Empty ─[uncached]───> Downloading ──> Indexing ──> Ready
 *                       ╰──────────────────────> Failed
 * ```
 *
 * - Empty: start state.
 * - Downloading: downloading filenames from the remote server. The filenames
 *                are cached using the browser's CacheStorage, if available.
 * - Indexing: processing the downloaded filenames. This step is usually
 *             instant, unless the repo is very large (>100k source files).
 *             In the torvalds/linux repo (~70k files), this step takes <1s
 *             on my computer but the chromium/chromium repo (~360k files)
 *             it takes ~3-5 seconds. This step is async so that the user can
 *             query against partially indexed results.
 * - Ready: all filenames have been indexed.
 * - Failed: something unexpected happened, the user can't fuzzy find files.
 */
export type FuzzyFSM = Empty | Downloading | Indexing | Ready | Failed
export interface Empty {
    key: 'empty'
}
export interface Downloading {
    key: 'downloading'
}
export interface Indexing {
    key: 'indexing'
    indexing: SearchIndexing
}
export interface Ready {
    key: 'ready'
    fuzzy: FuzzySearch
}
export interface Failed {
    key: 'failed'
    errorMessage: string
}

const FILE_NAMES = gql`
    query FileNames($repository: String!, $commit: String!) {
        repository(name: $repository) {
            id
            commit(rev: $commit) {
                id
                fileNames
            }
        }
    }
`

interface FilenameResult {
    downloadFilename: string[]
    isLoadingFilename: boolean
    filenameError: ApolloError | undefined
}

const useFilename = (repository: string, commit: string): FilenameResult => {
    const { data, loading, error } = useQuery<FileNamesResult, FileNamesVariables>(getDocumentNode(FILE_NAMES), {
        variables: { repository, commit },
    })

    return {
        downloadFilename: data?.repository?.commit?.fileNames || [],
        isLoadingFilename: loading,
        filenameError: error,
    }
}
