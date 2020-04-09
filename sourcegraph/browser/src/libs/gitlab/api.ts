import { first, identity } from 'lodash'
import { Observable, zip, of } from 'rxjs'
import { map, switchMap } from 'rxjs/operators'

import { memoizeObservable } from '../../../../shared/src/util/memoizeObservable'
import { GitLabInfo } from './scrape'
import { checkOk } from '../../../../shared/src/backend/fetch'
import { FileInfo } from '../code_intelligence'
import { Omit } from 'utility-types'
import { fromFetch } from '../../../../shared/src/graphql/fromFetch'

/**
 * Significant revisions for a merge request.
 */
interface DiffRefs {
    base_sha: string
    head_sha: string
    start_sha: string
}

/**
 * Response from the GitLab API for fetching a merge request. Note that there
 * is more information returned but we are not using it.
 */
interface MergeRequestResponse {
    diff_refs: DiffRefs
    source_project_id: string
}

/**
 * Response from the GitLab API for fetching a specific version(diff) of a merge
 * request. Note that there is more information returned but we are not using it.
 */
interface DiffVersionsResponse {
    base_commit_sha: string
}

const buildURL = (owner: string, projectName: string, path: string): string =>
    `${window.location.origin}/api/v4/projects/${encodeURIComponent(owner)}%2f${projectName}${path}`

const get = <T>(url: string): Observable<T> =>
    fromFetch(url, undefined, response => checkOk(response).json() as Promise<T>)

const getRepoNameFromProjectID = memoizeObservable(
    (projectId: string): Observable<string> =>
        get<{ web_url: string }>(`${window.location.origin}/api/v4/projects/${projectId}`).pipe(
            map(({ web_url }) => {
                const { hostname, pathname } = new URL(web_url)
                return `${hostname}${pathname}`
            })
        ),
    identity
)

/**
 * Fetches the base commit ID of the merge request at the given diffID.
 * If there is no diffID, emits `undefined`.
 */
const getBaseCommitIDFromDiffID = memoizeObservable(
    ({
        owner,
        projectName,
        mergeRequestID,
        diffID,
    }: Pick<GitLabInfo, 'owner' | 'projectName'> & { mergeRequestID: string; diffID?: string }): Observable<
        string | undefined
    > =>
        diffID
            ? get<DiffVersionsResponse>(
                  buildURL(owner, projectName, `/merge_requests/${mergeRequestID}/versions/${diffID}`)
              ).pipe(map(({ base_commit_sha }) => base_commit_sha))
            : of(undefined),
    ({ owner, projectName, mergeRequestID, diffID }) => `${owner}:${projectName}:${mergeRequestID}:${String(diffID)}`
)

/**
 * Fetches the fields of FileInfo common to all code views from the GitLab API.
 */
export const getMergeRequestDetailsFromAPI = memoizeObservable(
    ({
        owner,
        projectName,
        mergeRequestID,
        rawRepoName,
        diffID,
    }: Pick<GitLabInfo, 'owner' | 'projectName' | 'rawRepoName'> & {
        mergeRequestID: string
        diffID?: string
    }): Observable<Omit<FileInfo, 'filePath' | 'baseFilePath'>> =>
        zip(
            get<MergeRequestResponse>(buildURL(owner, projectName, `/merge_requests/${mergeRequestID}`)),
            getBaseCommitIDFromDiffID({ owner, projectName, mergeRequestID, diffID })
        ).pipe(
            switchMap(([{ diff_refs, source_project_id }, baseCommitIDFromDiffID]) =>
                getRepoNameFromProjectID(source_project_id).pipe(
                    map(
                        (baseRawRepoName): Omit<FileInfo, 'filePath' | 'baseFilePath'> => ({
                            baseCommitID: baseCommitIDFromDiffID || diff_refs.base_sha,
                            commitID: diff_refs.head_sha,
                            rawRepoName,
                            baseRawRepoName,
                        })
                    )
                )
            )
        ),
    ({ owner, projectName, mergeRequestID, rawRepoName, diffID }) =>
        `${owner}:${projectName}:${mergeRequestID}:${rawRepoName}:${String(diffID)}`
)

interface CommitResponse {
    parent_ids: string[]
}

/**
 * Get the base commit ID for a commit.
 */
export const getBaseCommitIDForCommit: ({
    owner,
    projectName,
    commitID,
}: Pick<GitLabInfo, 'owner' | 'projectName'> & { commitID: string }) => Observable<string> = memoizeObservable(
    ({ owner, projectName, commitID }) =>
        get<CommitResponse>(buildURL(owner, projectName, `/repository/commits/${commitID}`)).pipe(
            map(({ parent_ids }) => first(parent_ids)!) // ! because it'll always have a parent if we are looking at the commit page.
        ),
    ({ owner, projectName, commitID }) => `${owner}:${projectName}:${commitID}`
)
