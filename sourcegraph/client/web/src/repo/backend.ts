import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { createAggregateError, memoizeObservable } from '@sourcegraph/common'
import { dataOrThrowErrors, gql } from '@sourcegraph/http-client'
import {
    CloneInProgressError,
    RepoNotFoundError,
    RepoSeeOtherError,
    RevisionNotFoundError,
} from '@sourcegraph/shared/src/backend/errors'
import {
    makeRepoURI,
    RepoRevision,
    RepoSpec,
    ResolvedRevisionSpec,
    RevisionSpec,
} from '@sourcegraph/shared/src/util/url'

import { queryGraphQL, requestGraphQL } from '../backend/graphql'
import {
    ExternalLinkFields,
    FileExternalLinksResult,
    RepositoryFields,
    ResolveRepoRevResult,
} from '../graphql-operations'

export const externalLinkFieldsFragment = gql`
    fragment ExternalLinkFields on ExternalLink {
        url
        serviceKind
    }
`

export const repositoryFragment = gql`
    fragment RepositoryFields on Repository {
        id
        name
        url
        sourceType
        externalURLs {
            url
            serviceKind
        }
        externalRepository {
            serviceType
            serviceID
        }
        description
        viewerCanAdminister
        defaultBranch {
            displayName
            abbrevName
        }
        isFork
        metadata {
            key
            value
        }
    }
`

export interface ResolvedRevision extends ResolvedRevisionSpec {
    defaultBranch: string

    /** The URL to the repository root tree at the revision. */
    rootTreeURL: string
}

export interface Repo {
    repo: RepositoryFields
}

/**
 * When `revision` is undefined, the default branch is resolved.
 *
 * @returns Observable that emits the commit ID. Errors with a `CloneInProgressError` if the repo is still being cloned.
 */
export const resolveRepoRevision = memoizeObservable(
    ({ repoName, revision }: RepoSpec & Partial<RevisionSpec>): Observable<ResolvedRevision & Repo> =>
        queryGraphQL<ResolveRepoRevResult>(
            gql`
                query ResolveRepoRev($repoName: String!, $revision: String!) {
                    repositoryRedirect(name: $repoName) {
                        __typename
                        ... on Repository {
                            ...RepositoryFields
                            mirrorInfo {
                                cloneInProgress
                                cloneProgress
                                cloned
                            }
                            commit(rev: $revision) {
                                oid
                                tree(path: "") {
                                    url
                                }
                            }
                            defaultBranch {
                                abbrevName
                            }
                        }
                        ... on Redirect {
                            url
                        }
                    }
                }
                ${repositoryFragment}
            `,
            { repoName, revision: revision || '' }
        ).pipe(
            map(({ data, errors }) => {
                if (!data) {
                    throw createAggregateError(errors)
                }
                if (!data.repositoryRedirect) {
                    throw new RepoNotFoundError(repoName)
                }
                if (data.repositoryRedirect.__typename === 'Redirect') {
                    throw new RepoSeeOtherError(data.repositoryRedirect.url)
                }
                if (data.repositoryRedirect.mirrorInfo.cloneInProgress) {
                    throw new CloneInProgressError(
                        repoName,
                        data.repositoryRedirect.mirrorInfo.cloneProgress || undefined
                    )
                }
                if (!data.repositoryRedirect.mirrorInfo.cloned) {
                    throw new CloneInProgressError(repoName, 'queued for cloning')
                }
                if (!data.repositoryRedirect.commit) {
                    throw new RevisionNotFoundError(revision)
                }

                const defaultBranch = data.repositoryRedirect.defaultBranch?.abbrevName || 'HEAD'

                if (!data.repositoryRedirect.commit.tree) {
                    throw new RevisionNotFoundError(defaultBranch)
                }

                return {
                    repo: data.repositoryRedirect,
                    commitID: data.repositoryRedirect.commit.oid,
                    defaultBranch,
                    rootTreeURL: data.repositoryRedirect.commit.tree.url,
                }
            })
        ),
    makeRepoURI
)

export const fetchFileExternalLinks = memoizeObservable(
    (context: RepoRevision & { filePath: string }): Observable<ExternalLinkFields[]> =>
        queryGraphQL<FileExternalLinksResult>(
            gql`
                query FileExternalLinks($repoName: String!, $revision: String!, $filePath: String!) {
                    repository(name: $repoName) {
                        commit(rev: $revision) {
                            file(path: $filePath) {
                                externalURLs {
                                    ...ExternalLinkFields
                                }
                            }
                        }
                    }
                }

                ${externalLinkFieldsFragment}
            `,
            context
        ).pipe(
            map(({ data, errors }) => {
                if (!data?.repository?.commit?.file?.externalURLs) {
                    throw createAggregateError(errors)
                }
                return data.repository.commit.file.externalURLs
            })
        ),
    makeRepoURI
)

interface FetchCommitMessageResult {
    __typename?: 'Query'
    repository: {
        commit: {
            message: string
        }
    }
}

export const fetchCommitMessage = memoizeObservable(
    (context: RepoRevision): Observable<string> =>
        requestGraphQL<FetchCommitMessageResult, RepoRevision>(
            gql`
                query CommitMessage($repoName: String!, $revision: String!) {
                    repository(name: $repoName) {
                        commit(rev: $revision) {
                            message
                        }
                    }
                }
            `,
            context
        ).pipe(
            map(dataOrThrowErrors),
            map(data => data.repository.commit.message)
        ),
    makeRepoURI
)
