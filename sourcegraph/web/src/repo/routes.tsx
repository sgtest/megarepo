import React from 'react'
import { Redirect, RouteComponentProps } from 'react-router'
import { getModeFromPath } from '../../../shared/src/languages'
import { isLegacyFragment, parseHash } from '../../../shared/src/util/url'
import { formatHash } from '../util/url'
const BlobPage = React.lazy(async () => ({ default: (await import('./blob/BlobPage')).BlobPage }))
const RepositoryCommitsPage = React.lazy(async () => ({
    default: (await import('./commits/RepositoryCommitsPage')).RepositoryCommitsPage,
}))
const FilePathBreadcrumb = React.lazy(async () => ({
    default: (await import('./FilePathBreadcrumb')).FilePathBreadcrumb,
}))
import { RepoHeaderContributionPortal } from './RepoHeaderContributionPortal'
import { RepoRevContainerContext, RepoRevContainerRoute } from './RepoRevContainer'
const RepoRevSidebar = React.lazy(async () => ({ default: (await import('./RepoRevSidebar')).RepoRevSidebar }))
const TreePage = React.lazy(async () => ({ default: (await import('./TreePage')).TreePage }))

/** Dev feature flag to make benchmarking the file tree in isolation easier. */
const hideRepoRevContent = localStorage.getItem('hideRepoRevContent')

export const repoRevContainerRoutes: ReadonlyArray<RepoRevContainerRoute> = [
    ...['', '/-/:objectType(blob|tree)/:filePath+'].map(routePath => ({
        path: routePath,
        exact: routePath === '',
        render: ({
            repo: { name: repoName, id: repoID, description: repoDescription },
            resolvedRev: { commitID, defaultBranch },
            match,
            ...context
        }: RepoRevContainerContext &
            RouteComponentProps<{
                objectType: 'blob' | 'tree' | undefined
                filePath: string | undefined
            }>) => {
            const objectType: 'blob' | 'tree' = match.params.objectType || 'tree'
            const filePath = match.params.filePath || '' // empty string is root
            const mode = getModeFromPath(filePath)

            // For blob pages with legacy URL fragment hashes like "#L17:19-21:23$foo:bar"
            // redirect to the modern URL fragment hashes like "#L17:19-21:23&tab=foo:bar"
            if (!hideRepoRevContent && objectType === 'blob' && isLegacyFragment(window.location.hash)) {
                const hash = parseHash(window.location.hash)
                const newHash = new URLSearchParams()
                if (hash.viewState) {
                    newHash.set('tab', hash.viewState)
                }
                return <Redirect to={window.location.pathname + window.location.search + formatHash(hash, newHash)} />
            }

            const repoRevProps = { repoID, repoDescription, repoName, commitID }

            return (
                <>
                    {filePath && (
                        <>
                            <RepoHeaderContributionPortal
                                position="nav"
                                priority={10}
                                element={
                                    <FilePathBreadcrumb
                                        key="path"
                                        repoName={repoName}
                                        rev={context.rev}
                                        filePath={filePath}
                                        isDir={objectType === 'tree'}
                                    />
                                }
                                repoHeaderContributionsLifecycleProps={context.repoHeaderContributionsLifecycleProps}
                            />
                        </>
                    )}
                    <RepoRevSidebar
                        {...context}
                        {...repoRevProps}
                        className="repo-rev-container__sidebar"
                        filePath={match.params.filePath || '' || ''}
                        isDir={objectType === 'tree'}
                        defaultBranch={defaultBranch || 'HEAD'}
                    />
                    {!hideRepoRevContent && (
                        <div className="repo-rev-container__content">
                            {objectType === 'blob' ? (
                                <BlobPage
                                    {...context}
                                    {...repoRevProps}
                                    filePath={match.params.filePath || ''}
                                    mode={mode}
                                    repoHeaderContributionsLifecycleProps={
                                        context.repoHeaderContributionsLifecycleProps
                                    }
                                />
                            ) : (
                                <TreePage {...context} {...repoRevProps} filePath={match.params.filePath || ''} />
                            )}
                        </div>
                    )}
                </>
            )
        },
    })),
    {
        path: '/-/commits',
        render: ({ resolvedRev: { commitID }, repoHeaderContributionsLifecycleProps, ...context }) => (
            <RepositoryCommitsPage
                {...context}
                commitID={commitID}
                repoHeaderContributionsLifecycleProps={repoHeaderContributionsLifecycleProps}
            />
        ),
    },
]
