import React from 'react'

import { RouteComponentProps } from 'react-router-dom'

import { TraceSpanProvider } from '@sourcegraph/observability-client'
import { lazyComponent } from '@sourcegraph/shared/src/util/lazyComponent'

import { ActionItemsBarProps } from '../extensions/components/ActionItemsBar'

import type { RepositoryCommitsPageProps } from './commits/RepositoryCommitsPage'
import { RepoRevisionWrapper } from './components/RepoRevision'
import { RepoContainerRoute } from './RepoContainer'
import { RepoRevisionContainerContext, RepoRevisionContainerRoute } from './RepoRevisionContainer'
import { RepositoryFileTreePageProps } from './RepositoryFileTreePage'
import { RepositoryTagTab } from './tree/TagTab'

const RepositoryCommitsPage = lazyComponent(() => import('./commits/RepositoryCommitsPage'), 'RepositoryCommitsPage')

const RepositoryFileTreePage = lazyComponent(() => import('./RepositoryFileTreePage'), 'RepositoryFileTreePage')

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
const RepositoryBranchesTab = lazyComponent(() => import('./tree/BranchesTab'), 'RepositoryBranchesTab')
const ActionItemsBar = lazyComponent<ActionItemsBarProps, 'ActionItemsBar'>(
    () => import('../extensions/components/ActionItemsBar'),
    'ActionItemsBar'
)

export const compareSpecPath = '/-/compare/:spec*'

export const repoContainerRoutes: readonly RepoContainerRoute[] = [
    {
        path: '/-/commit/:revspec+',
        render: context => (
            <RepoRevisionWrapper>
                <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                    <RepositoryCommitPage {...context} />
                </RepositoryGitDataContainer>
                {window.context.enableLegacyExtensions && (
                    <ActionItemsBar
                        extensionsController={context.extensionsController}
                        platformContext={context.platformContext}
                        useActionItemsBar={context.useActionItemsBar}
                        location={context.location}
                        telemetryService={context.telemetryService}
                        source="commit"
                    />
                )}
            </RepoRevisionWrapper>
        ),
    },
    {
        path: '/-/branches',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                <RepositoryBranchesArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: '/-/tags',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                <RepositoryReleasesArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: compareSpecPath,
        render: context => (
            <RepoRevisionWrapper>
                <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                    <RepositoryCompareArea {...context} />
                </RepositoryGitDataContainer>
                {window.context.enableLegacyExtensions && (
                    <ActionItemsBar
                        extensionsController={context.extensionsController}
                        platformContext={context.platformContext}
                        useActionItemsBar={context.useActionItemsBar}
                        location={context.location}
                        telemetryService={context.telemetryService}
                        source="compare"
                    />
                )}
            </RepoRevisionWrapper>
        ),
    },
    {
        path: '/-/stats',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                <RepositoryStatsArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
    {
        path: '/-/settings',
        render: context => (
            <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                <RepoSettingsArea {...context} />
            </RepositoryGitDataContainer>
        ),
    },
]

export const RepoContributors: React.FunctionComponent<
    React.PropsWithChildren<RepoRevisionContainerContext & RouteComponentProps>
> = ({ useBreadcrumb, setBreadcrumb, repo, history, location, match, globbing, repoName }) => (
    <RepositoryStatsArea
        useBreadcrumb={useBreadcrumb}
        setBreadcrumb={setBreadcrumb}
        repo={repo}
        repoName={repoName}
        history={history}
        location={location}
        match={match}
        globbing={globbing}
    />
)

export const RepoCommits: React.FunctionComponent<
    Omit<RepositoryCommitsPageProps, 'repo'> & Pick<RepoRevisionContainerContext, 'repo'> & RouteComponentProps
> = ({ revision, repo, ...context }) => <RepositoryCommitsPage {...context} repo={repo} revision={revision} />

const blobPath = '/-/:objectType(blob)/:filePath*'
const treePath = '/-/:objectType(tree)/:filePath*'
export const commitsPath = '/-/commits'

export const repoRevisionContainerRoutes: readonly RepoRevisionContainerRoute[] = [
    ...[
        '',
        blobPath,
        treePath,
        '/-/docs/tab/:pathID*',
        '/-/commits/tab',
        '/-/branch/tab',
        '/-/tag/tab',
        '/-/contributors/tab',
        '/-/compare/tab/:spec*',
    ].map(routePath => ({
        path: routePath,
        exact: routePath === '',
        render: (props: RepositoryFileTreePageProps) => (
            <TraceSpanProvider
                name="RepositoryFileTreePage"
                attributes={{
                    objectType: props.match.params.objectType,
                }}
            >
                <RepositoryFileTreePage {...props} />
                <ActionItemsBar
                    repo={props.repo}
                    useActionItemsBar={props.useActionItemsBar}
                    location={props.location}
                    extensionsController={props.extensionsController}
                    platformContext={props.platformContext}
                    telemetryService={props.telemetryService}
                    source={routePath === blobPath ? 'blob' : undefined}
                />
            </TraceSpanProvider>
        ),
    })),
    {
        path: commitsPath,
        render: RepoCommits,
    },
    {
        path: '/-/branch',
        render: ({ repo, location, history }) => (
            <RepositoryBranchesTab repo={repo} location={location} history={history} />
        ),
    },
    {
        path: '/-/tag',
        render: ({ repo, location, history }) => <RepositoryTagTab repo={repo} location={location} history={history} />,
    },
    {
        path: compareSpecPath,
        render: context => (
            <RepoRevisionWrapper>
                <RepositoryGitDataContainer {...context} repoName={context.repoName}>
                    <RepositoryCompareArea {...context} />
                </RepositoryGitDataContainer>
                {window.context.enableLegacyExtensions && (
                    <ActionItemsBar
                        extensionsController={context.extensionsController}
                        platformContext={context.platformContext}
                        useActionItemsBar={context.useActionItemsBar}
                        location={context.location}
                        telemetryService={context.telemetryService}
                        source="compare"
                    />
                )}
            </RepoRevisionWrapper>
        ),
    },
    {
        path: '/-/contributors',
        render: RepoContributors,
    },
]
