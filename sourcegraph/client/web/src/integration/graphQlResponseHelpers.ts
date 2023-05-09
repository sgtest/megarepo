import { encodeURIPathComponent } from '@sourcegraph/common'
import { JsonDocument } from '@sourcegraph/shared/src/codeintel/scip'
import { RepositoryType, TreeEntriesResult } from '@sourcegraph/shared/src/graphql-operations'

import {
    BlobResult,
    ExternalServiceKind,
    FileExternalLinksResult,
    FileNamesResult,
    FileTreeEntriesResult,
    RepoChangesetsStatsResult,
    ResolveRepoRevResult,
} from '../graphql-operations'

export const createTreeEntriesResult = (url: string, toplevelFiles: string[]): TreeEntriesResult => ({
    repository: {
        id: `$repo-id-${url}`,
        commit: {
            tree: {
                isRoot: true,
                url,
                entries: toplevelFiles.map(name => ({
                    name,
                    path: name,
                    isDirectory: false,
                    url: `${url}/-/blob/${name}`,
                    submodule: null,
                    isSingleChild: false,
                })),
            },
        },
    },
})

export const createFileTreeEntriesResult = (url: string, toplevelFiles: string[]): FileTreeEntriesResult =>
    createTreeEntriesResult(url, toplevelFiles)

export const createBlobContentResult = (content: string, lsif?: JsonDocument): BlobResult => ({
    repository: {
        commit: {
            file: {
                __typename: 'VirtualFile',
                content,
                richHTML: '',
                totalLines: content.split('\n').length,
                highlight: {
                    aborted: false,
                    lsif: lsif ? JSON.stringify(lsif) : '',
                },
            },
        },
    },
})

export const createFileExternalLinksResult = (
    url: string,
    serviceKind: ExternalServiceKind = ExternalServiceKind.GITHUB
): FileExternalLinksResult => ({
    repository: {
        commit: {
            file: {
                externalURLs: [{ url, serviceKind }],
            },
        },
    },
})

export const createRepoChangesetsStatsResult = (): RepoChangesetsStatsResult => ({
    repository: {
        id: 'a',
        changesetsStats: {
            open: 2,
            merged: 4,
        },
    },
})

export const createResolveRepoRevisionResult = (treeUrl: string, oid = '1'.repeat(40)): ResolveRepoRevResult => ({
    repositoryRedirect: {
        __typename: 'Repository',
        id: `RepositoryID:${treeUrl}`,
        name: treeUrl,
        url: `/${encodeURIPathComponent(treeUrl)}`,
        sourceType: RepositoryType.GIT_REPOSITORY,
        externalURLs: [
            {
                url: new URL(`https://${encodeURIPathComponent(treeUrl)}`).href,
                serviceKind: ExternalServiceKind.GITHUB,
            },
        ],
        externalRepository: { serviceType: 'github', serviceID: 'https://github.com/' },
        description: 'bla',
        viewerCanAdminister: false,
        defaultBranch: { displayName: 'master', abbrevName: 'master' },
        mirrorInfo: { cloneInProgress: false, cloneProgress: '', cloned: true },
        commit: {
            oid,
            tree: { url: '/' + treeUrl },
        },
        isFork: false,
        metadata: [],
    },
})

export const createResolveCloningRepoRevisionResult = (
    treeUrl: string
): ResolveRepoRevResult & { errors: { message: string }[] } => ({
    repositoryRedirect: {
        __typename: 'Repository',
        id: `RepositoryID:${treeUrl}`,
        name: treeUrl,
        url: `/${encodeURIPathComponent(treeUrl)}`,
        sourceType: RepositoryType.GIT_REPOSITORY,
        externalURLs: [
            {
                url: new URL(`https://${encodeURIPathComponent(treeUrl)}`).href,
                serviceKind: ExternalServiceKind.GITHUB,
            },
        ],
        externalRepository: { serviceType: 'github', serviceID: 'https://github.com/' },
        description: 'bla',
        viewerCanAdminister: false,
        defaultBranch: null,
        mirrorInfo: {
            cloneInProgress: true,
            cloneProgress: 'starting clone',
            cloned: false,
        },
        commit: null,
        isFork: false,
        metadata: [],
    },
    errors: [
        {
            message: `repository does not exist (clone in progress): ${treeUrl}`,
        },
    ],
})

export const createFileNamesResult = (): FileNamesResult => ({
    repository: {
        id: 'repo-123',
        __typename: 'Repository',
        commit: { id: 'c0ff33', __typename: 'GitCommit', fileNames: ['README.md'] },
    },
})
