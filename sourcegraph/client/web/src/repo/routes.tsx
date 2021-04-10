import React from 'react'
import { Redirect, RouteComponentProps } from 'react-router'

import { getModeFromPath } from '@sourcegraph/shared/src/languages'
import { isLegacyFragment, parseHash, toRepoURL } from '@sourcegraph/shared/src/util/url'

import { ErrorBoundary } from '../components/ErrorBoundary'
import { ActionItemsBar } from '../extensions/components/ActionItemsBar'
import { lazyComponent } from '../util/lazyComponent'
import { formatHash } from '../util/url'

import { RepoContainerRoute } from './RepoContainer'
import { RepoRevisionContainerContext, RepoRevisionContainerRoute } from './RepoRevisionContainer'

const BlobPage = lazyComponent(() => import('./blob/BlobPage'), 'BlobPage')
const RepositoryCommitsPage = lazyComponent(() => import('./commits/RepositoryCommitsPage'), 'RepositoryCommitsPage')
const RepoRevisionSidebar = lazyComponent(() => import('./RepoRevisionSidebar'), 'RepoRevisionSidebar')
const TreePage = lazyComponent(() => import('./tree/TreePage'), 'TreePage')

const RepositoryGitDataContainer = lazyComponent(
    () => import('./RepositoryGitDataContainer'),
    'RepositoryGitDataContainer'
)
const RepositoryCommitPage = lazyComponent(() => import('./commit/RepositoryCommitPage'), 'RepositoryCommitPage')
const RepositoryBranchesArea = lazyComponent(
    () => import('./branches/RepositoryBranchesArea'),
    'RepositoryBranchesArea'
)
const RepositoryReleasesArea = lazyComponent(
    () => import('./releases/RepositoryReleasesArea'),
    'RepositoryReleasesArea'
)
const RepoSettingsArea = lazyComponent(() => import('./settings/RepoSettingsArea'), 'RepoSettingsArea')
const RepositoryCompareArea = lazyComponent(() => import('./compare/RepositoryCompareArea'), 'RepositoryCompareArea')
const RepositoryStatsArea = lazyComponent(() => import('./stats/RepositoryStatsArea'), 'RepositoryStatsArea')

export const repoContainerRoutes: readonly RepoContainerRoute[] = [
    {
        path: '/-/commit/:revspec+',
        render: context => (
            <div className="repo-revision-container">
                <RepositoryGitDataContainer {...context} repoName={context.repo.name}>
                    <RepositoryCommitPage {...context} />
                </RepositoryGitDataContainer>
                <ActionItemsBar
                    extensionsController={context.extensionsController}
                    platformContext={context.platformContext}
                    useActionItemsBar={context.useActionItemsBar}
                    location={context.location}
                    telemetryService={context.telemetryService}
                />
            </div>
        ),
    },
    {
        path: '/-/branches',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repo.name}>
                <RepositoryBranchesArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: '/-/tags',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repo.name}>
                <RepositoryReleasesArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: '/-/compare/:spec*',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repo.name}>
                <RepositoryCompareArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: '/-/stats',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repo.name}>
                <RepositoryStatsArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: '/-/settings',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repo.name}>
                <RepoSettingsArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
]

/** Dev feature flag to make benchmarking the file tree in isolation easier. */
const hideRepoRevisionContent = localStorage.getItem('hideRepoRevContent')

export const repoRevisionContainerRoutes: readonly RepoRevisionContainerRoute[] = [
    ...['', '/-/:objectType(blob|tree)/:filePath*'].map(routePath => ({
        path: routePath,
        exact: routePath === '',
        render: ({
            repo,
            resolvedRev: { commitID, defaultBranch },
            match,
            patternType,
            setPatternType,
            caseSensitive,
            setCaseSensitivity,
            copyQueryButton,
            versionContext,
            globbing,
            ...context
        }: RepoRevisionContainerContext &
            RouteComponentProps<{
                objectType: 'blob' | 'tree' | undefined
                filePath: string | undefined
            }>) => {
            // The decoding depends on the pinned `history` version.
            // See https://github.com/sourcegraph/sourcegraph/issues/4408
            // and https://github.com/ReactTraining/history/issues/505
            const filePath = decodeURIComponent(match.params.filePath || '') // empty string is root

            // Redirect tree and blob routes pointing to the root to the repo page
            if (match.params.objectType && filePath.replace(/\/+$/g, '') === '') {
                return <Redirect to={toRepoURL({ repoName: repo.name, revision: context.revision })} />
            }

            const objectType: 'blob' | 'tree' = match.params.objectType || 'tree'

            const mode = getModeFromPath(filePath)

            // For blob pages with legacy URL fragment hashes like "#L17:19-21:23$foo:bar"
            // redirect to the modern URL fragment hashes like "#L17:19-21:23&tab=foo:bar"
            if (!hideRepoRevisionContent && objectType === 'blob' && isLegacyFragment(window.location.hash)) {
                const hash = parseHash(window.location.hash)
                const newHash = new URLSearchParams()
                if (hash.viewState) {
                    newHash.set('tab', hash.viewState)
                }
                return <Redirect to={window.location.pathname + window.location.search + formatHash(hash, newHash)} />
            }

            const repoRevisionProps = {
                commitID,
                filePath,
                patternType,
                setPatternType,
                caseSensitive,
                setCaseSensitivity,
                copyQueryButton,
                versionContext,
                globbing,
            }

            return (
                <>
                    <RepoRevisionSidebar
                        {...context}
                        {...repoRevisionProps}
                        repoID={repo.id}
                        repoName={repo.name}
                        className="repo-revision-container__sidebar"
                        isDir={objectType === 'tree'}
                        defaultBranch={defaultBranch || 'HEAD'}
                    />
                    {!hideRepoRevisionContent && (
                        <div className="repo-revision-container__content">
                            <ErrorBoundary location={context.location}>
                                {objectType === 'blob' ? (
                                    <BlobPage
                                        {...context}
                                        {...repoRevisionProps}
                                        repoID={repo.id}
                                        repoName={repo.name}
                                        mode={mode}
                                        repoHeaderContributionsLifecycleProps={
                                            context.repoHeaderContributionsLifecycleProps
                                        }
                                    />
                                ) : (
                                    <TreePage {...context} {...repoRevisionProps} repo={repo} />
                                )}
                            </ErrorBoundary>
                        </div>
                    )}
                    <ActionItemsBar
                        useActionItemsBar={context.useActionItemsBar}
                        location={context.location}
                        extensionsController={context.extensionsController}
                        platformContext={context.platformContext}
                        telemetryService={context.telemetryService}
                    />
                </>
            )
        },
    })),
    {
        path: '/-/commits',
        render: ({ resolvedRev: { commitID }, repoHeaderContributionsLifecycleProps, ...context }) => (
            <>
                <RepositoryCommitsPage
                    {...context}
                    commitID={commitID}
                    repoHeaderContributionsLifecycleProps={repoHeaderContributionsLifecycleProps}
                />
                <ActionItemsBar
                    useActionItemsBar={context.useActionItemsBar}
                    location={context.location}
                    extensionsController={context.extensionsController}
                    platformContext={context.platformContext}
                    telemetryService={context.telemetryService}
                />
            </>
        ),
    },
]
