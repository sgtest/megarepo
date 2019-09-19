import { first } from 'lodash'
import { Observable } from 'rxjs'
import { map, switchMap } from 'rxjs/operators'

import { memoizeObservable } from '../../../../shared/src/util/memoizeObservable'
import { GitLabDiffInfo } from './scrape'
import { fromFetch } from 'rxjs/fetch'
import { checkOk } from '../../../../shared/src/backend/fetch'

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
}

/**
 * Response from the GitLab API for fetching a specific version(diff) of a merge
 * request. Note that there is more information returned but we are not using it.
 */
interface DiffVersionsResponse {
    base_commit_sha: string
}

type GetBaseCommitIDInput = Pick<GitLabDiffInfo, 'owner' | 'projectName' | 'mergeRequestID' | 'diffID'>

const buildURL = (owner: string, projectName: string, path: string): string =>
    `${window.location.origin}/api/v4/projects/${encodeURIComponent(owner)}%2f${projectName}${path}`

const get = <T>(url: string): Observable<T> =>
    fromFetch(url).pipe(
        map(checkOk),
        switchMap(response => response.json())
    )

/**
 * Get the base commit ID for a merge request.
 */
export const getBaseCommitIDForMergeRequest: (info: GetBaseCommitIDInput) => Observable<string> = memoizeObservable(
    ({ owner, projectName, mergeRequestID, diffID }: GetBaseCommitIDInput) => {
        const mrURL = buildURL(owner, projectName, `/merge_requests/${mergeRequestID}`)

        // If we have a `diffID`, retrieve the information for that individual diff.
        if (diffID) {
            return get<DiffVersionsResponse>(`${mrURL}/versions/${diffID}`).pipe(
                map(({ base_commit_sha }) => base_commit_sha)
            )
        }

        // Otherwise, just get the overall base `commitID` for the merge request.
        return get<MergeRequestResponse>(mrURL).pipe(map(({ diff_refs: { base_sha } }) => base_sha))
    },
    ({ mergeRequestID, diffID }) => mergeRequestID + (diffID ? `/${diffID}` : '')
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
}: Pick<GetBaseCommitIDInput, 'owner' | 'projectName'> & { commitID: string }) => Observable<
    string
> = memoizeObservable(
    ({ owner, projectName, commitID }) =>
        get<CommitResponse>(buildURL(owner, projectName, `/repository/commits/${commitID}`)).pipe(
            map(({ parent_ids }) => first(parent_ids)!) // ! because it'll always have a parent if we are looking at the commit page.
        ),
    ({ owner, projectName, commitID }) => `${owner}:${projectName}:${commitID}`
)
