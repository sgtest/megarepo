import { dirname } from 'path'

import { browser } from '$app/environment'
import { fetchSidebarFileTree } from '$lib/repo/api/tree'

import type { LayoutLoad } from './$types'
import { GitHistoryQuery } from './layout.gql'

const HISTORY_COMMITS_PER_PAGE = 20

// Signifies the path of the repository root
const REPO_ROOT = '.'

let getRootPath = (_repo: string, path: string) => path

// We keep state in the browser to load the tree entries of the "highest" directory that was visited.
if (browser) {
    const topTreePath: Record<string, string> = {}

    getRootPath = (repo: string, path: string) => {
        const treePath = topTreePath[repo]
        if (treePath && (treePath === REPO_ROOT || path.startsWith(treePath))) {
            return topTreePath[repo]
        }
        return (topTreePath[repo] = path)
    }
}

export const load: LayoutLoad = async ({ parent, params }) => {
    const { resolvedRevision, repoName, graphqlClient } = await parent()
    const parentPath = getRootPath(repoName, params.path ? dirname(params.path) : REPO_ROOT)

    // Fetches the most recent commits for current blob, tree or repo root
    const commitHistory = graphqlClient.watchQuery({
        query: GitHistoryQuery,
        variables: {
            repo: resolvedRevision.repo.id,
            revspec: resolvedRevision.commitID,
            filePath: params.path ?? '',
            first: HISTORY_COMMITS_PER_PAGE,
            afterCursor: null,
        },
        notifyOnNetworkStatusChange: true,
    })
    if (!graphqlClient.readQuery({ query: GitHistoryQuery, variables: commitHistory.variables })) {
        // Eagerly fetch data if it isn't in the cache already. This ensures that the data is fetched
        // as soon as possible, not only after the layout subscribes to the query.
        commitHistory.refetch()
    }

    return {
        parentPath,
        commitHistory,
        deferred: {
            fileTree: fetchSidebarFileTree({
                repoID: resolvedRevision.repo.id,
                commitID: resolvedRevision.commitID,
                filePath: parentPath,
            }),
        },
    }
}
