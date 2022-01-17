import classNames from 'classnames'
import { subYears, formatISO } from 'date-fns'
import * as H from 'history'
import AccountIcon from 'mdi-react/AccountIcon'
import BookOpenBlankVariantIcon from 'mdi-react/BookOpenBlankVariantIcon'
import BrainIcon from 'mdi-react/BrainIcon'
import FolderIcon from 'mdi-react/FolderIcon'
import HistoryIcon from 'mdi-react/HistoryIcon'
import SettingsIcon from 'mdi-react/SettingsIcon'
import SourceBranchIcon from 'mdi-react/SourceBranchIcon'
import SourceCommitIcon from 'mdi-react/SourceCommitIcon'
import SourceRepositoryIcon from 'mdi-react/SourceRepositoryIcon'
import TagIcon from 'mdi-react/TagIcon'
import React, { useState, useMemo, useCallback, useEffect } from 'react'
import { Link, Redirect } from 'react-router-dom'
import { Observable, EMPTY } from 'rxjs'
import { catchError, map } from 'rxjs/operators'

import { asError, ErrorLike, isErrorLike } from '@sourcegraph/common'
import { gql, dataOrThrowErrors } from '@sourcegraph/http-client'
import { ActionItem } from '@sourcegraph/shared/src/actions/ActionItem'
import { ActionsContainer } from '@sourcegraph/shared/src/actions/ActionsContainer'
import { FileDecorationsByPath } from '@sourcegraph/shared/src/api/extension/extensionHostApi'
import { ContributableMenu } from '@sourcegraph/shared/src/api/protocol'
import { ActivationProps } from '@sourcegraph/shared/src/components/activation/Activation'
import { displayRepoName } from '@sourcegraph/shared/src/components/RepoFileLink'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import * as GQL from '@sourcegraph/shared/src/schema'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { memoizeObservable } from '@sourcegraph/shared/src/util/memoizeObservable'
import { pluralize } from '@sourcegraph/shared/src/util/strings'
import { encodeURIPathComponent, toPrettyBlobURL, toURIWithPath } from '@sourcegraph/shared/src/util/url'
import { Container, PageHeader, LoadingSpinner, Button, useObservable } from '@sourcegraph/wildcard'

import { getFileDecorations } from '../../backend/features'
import { queryGraphQL } from '../../backend/graphql'
import { BatchChangesProps } from '../../batches'
import { RepoBatchChangesButton } from '../../batches/RepoBatchChangesButton'
import { CodeIntelligenceProps } from '../../codeintel'
import { ErrorAlert } from '../../components/alerts'
import { BreadcrumbSetters } from '../../components/Breadcrumbs'
import { FilteredConnection } from '../../components/FilteredConnection'
import { PageTitle } from '../../components/PageTitle'
import { GitCommitFields, Scalars, TreePageRepositoryFields } from '../../graphql-operations'
import { CodeInsightsProps } from '../../insights/types'
import { Settings } from '../../schema/settings.schema'
import { SearchContextProps } from '../../search'
import { useExperimentalFeatures } from '../../stores'
import { basename } from '../../util/path'
import { fetchTreeEntries } from '../backend'
import { GitCommitNode, GitCommitNodeProps } from '../commits/GitCommitNode'
import { gitCommitFragment } from '../commits/RepositoryCommitsPage'
import { FilePathBreadcrumbs } from '../FilePathBreadcrumbs'

import { TreeEntriesSection } from './TreeEntriesSection'
import styles from './TreePage.module.scss'

const fetchTreeCommits = memoizeObservable(
    (args: {
        repo: Scalars['ID']
        revspec: string
        first?: number
        filePath?: string
        after?: string
    }): Observable<GQL.IGitCommitConnection> =>
        queryGraphQL(
            gql`
                query TreeCommits($repo: ID!, $revspec: String!, $first: Int, $filePath: String, $after: String) {
                    node(id: $repo) {
                        __typename
                        ... on Repository {
                            commit(rev: $revspec) {
                                ancestors(first: $first, path: $filePath, after: $after) {
                                    nodes {
                                        ...GitCommitFields
                                    }
                                    pageInfo {
                                        hasNextPage
                                    }
                                }
                            }
                        }
                    }
                }
                ${gitCommitFragment}
            `,
            args
        ).pipe(
            map(dataOrThrowErrors),
            map(data => {
                if (!data.node) {
                    throw new Error('Repository not found')
                }
                if (data.node.__typename !== 'Repository') {
                    throw new Error('Node is not a Repository')
                }
                if (!data.node.commit) {
                    throw new Error('Commit not found')
                }
                return data.node.commit.ancestors
            })
        ),
    args => `${args.repo}:${args.revspec}:${String(args.first)}:${String(args.filePath)}:${String(args.after)}`
)

interface Props
    extends SettingsCascadeProps<Settings>,
        ExtensionsControllerProps,
        PlatformContextProps,
        ThemeProps,
        TelemetryProps,
        ActivationProps,
        CodeIntelligenceProps,
        BatchChangesProps,
        CodeInsightsProps,
        Pick<SearchContextProps, 'selectedSearchContextSpec'>,
        BreadcrumbSetters {
    repo: TreePageRepositoryFields
    /** The tree's path in TreePage. We call it filePath for consistency elsewhere. */
    filePath: string
    commitID: string
    revision: string
    location: H.Location
    history: H.History
    globbing: boolean
}

export const treePageRepositoryFragment = gql`
    fragment TreePageRepositoryFields on Repository {
        id
        name
        description
        viewerCanAdminister
        url
    }
`

export const TreePage: React.FunctionComponent<Props> = ({
    repo,
    commitID,
    revision,
    filePath,
    settingsCascade,
    useBreadcrumb,
    codeIntelligenceEnabled,
    batchChangesEnabled,
    extensionViews: ExtensionViewsSection,
    ...props
}) => {
    useEffect(() => {
        if (filePath === '') {
            props.telemetryService.logViewEvent('Repository')
        } else {
            props.telemetryService.logViewEvent('Tree')
        }
    }, [filePath, props.telemetryService])

    useBreadcrumb(
        useMemo(() => {
            if (!filePath) {
                return
            }
            return {
                key: 'treePath',
                className: 'flex-shrink-past-contents',
                element: (
                    <FilePathBreadcrumbs
                        key="path"
                        repoName={repo.name}
                        revision={revision}
                        filePath={filePath}
                        isDir={true}
                        repoUrl={repo.url}
                        telemetryService={props.telemetryService}
                    />
                ),
            }
        }, [repo.name, repo.url, revision, filePath, props.telemetryService])
    )

    const [showOlderCommits, setShowOlderCommits] = useState(false)

    const onShowOlderCommitsClicked = useCallback(
        (event: React.MouseEvent): void => {
            event.preventDefault()
            setShowOlderCommits(true)
        },
        [setShowOlderCommits]
    )

    const treeOrError = useObservable(
        useMemo(
            () =>
                fetchTreeEntries({
                    repoName: repo.name,
                    commitID,
                    revision,
                    filePath,
                    first: 2500,
                }).pipe(catchError((error): [ErrorLike] => [asError(error)])),
            [repo.name, commitID, revision, filePath]
        )
    )

    const fileDecorationsByPath =
        useObservable<FileDecorationsByPath>(
            useMemo(
                () =>
                    treeOrError && !isErrorLike(treeOrError)
                        ? getFileDecorations({
                              files: treeOrError.entries,
                              extensionsController: props.extensionsController,
                              repoName: repo.name,
                              commitID,
                              parentNodeUri: treeOrError.url,
                          })
                        : EMPTY,
                [treeOrError, repo.name, commitID, props.extensionsController]
            )
        ) ?? {}

    const showCodeInsights =
        !isErrorLike(settingsCascade.final) &&
        !!settingsCascade.final?.experimentalFeatures?.codeInsights &&
        settingsCascade.final['insights.displayLocation.directory'] === true

    // Add DirectoryViewer
    const uri = toURIWithPath({ repoName: repo.name, commitID, filePath })

    useEffect(() => {
        if (!showCodeInsights) {
            return
        }

        const viewerIdPromise = props.extensionsController.extHostAPI
            .then(extensionHostAPI =>
                extensionHostAPI.addViewerIfNotExists({
                    type: 'DirectoryViewer',
                    isActive: true,
                    resource: uri,
                })
            )
            .catch(error => {
                console.error('Error adding viewer to extension host:', error)
                return null
            })

        return () => {
            Promise.all([props.extensionsController.extHostAPI, viewerIdPromise])
                .then(([extensionHostAPI, viewerId]) => {
                    if (viewerId) {
                        return extensionHostAPI.removeViewer(viewerId)
                    }
                    return
                })
                .catch(error => console.error('Error removing viewer from extension host:', error))
        }
    }, [uri, showCodeInsights, props.extensionsController])

    // eslint-disable-next-line unicorn/prevent-abbreviations
    const enableAPIDocs = useExperimentalFeatures(features => features.apiDocs)

    const getPageTitle = (): string => {
        const repoString = displayRepoName(repo.name)
        if (filePath) {
            return `${basename(filePath)} - ${repoString}`
        }
        return `${repoString}`
    }

    const queryCommits = useCallback(
        (args: { first?: number }): Observable<GQL.IGitCommitConnection> => {
            const after: string | undefined = showOlderCommits ? undefined : formatISO(subYears(Date.now(), 1))
            return fetchTreeCommits({
                ...args,
                repo: repo.id,
                revspec: revision || '',
                filePath,
                after,
            })
        },
        [filePath, repo.id, revision, showOlderCommits]
    )

    const emptyElement = showOlderCommits ? (
        <>No commits in this tree.</>
    ) : (
        <div className="test-tree-page-no-recent-commits">
            <p className="mb-2">No commits in this tree in the past year.</p>
            <Button
                className="test-tree-page-show-all-commits"
                onClick={onShowOlderCommitsClicked}
                variant="secondary"
                size="sm"
            >
                Show all commits
            </Button>
        </div>
    )

    const TotalCountSummary: React.FunctionComponent<{ totalCount: number }> = ({ totalCount }) => (
        <div className="mt-2">
            {showOlderCommits ? (
                <>
                    {totalCount} total {pluralize('commit', totalCount)} in this tree.
                </>
            ) : (
                <>
                    <p className="mb-2">
                        {totalCount} {pluralize('commit', totalCount)} in this tree in the past year.
                    </p>
                    <Button onClick={onShowOlderCommitsClicked} variant="secondary" size="sm">
                        Show all commits
                    </Button>
                </>
            )}
        </div>
    )

    return (
        <div className={styles.treePage}>
            <Container className={styles.container}>
                <PageTitle title={getPageTitle()} />
                {treeOrError === undefined ? (
                    <div>
                        <LoadingSpinner /> Loading files and directories
                    </div>
                ) : isErrorLike(treeOrError) ? (
                    // If the tree is actually a blob, be helpful and redirect to the blob page.
                    // We don't have error names on GraphQL errors.
                    /not a directory/i.test(treeOrError.message) ? (
                        <Redirect to={toPrettyBlobURL({ repoName: repo.name, revision, commitID, filePath })} />
                    ) : (
                        <ErrorAlert error={treeOrError} />
                    )
                ) : (
                    <>
                        <header className="mb-3">
                            {treeOrError.isRoot ? (
                                <>
                                    <PageHeader
                                        path={[{ icon: SourceRepositoryIcon, text: displayRepoName(repo.name) }]}
                                        className="mb-3 test-tree-page-title"
                                    />
                                    {repo.description && <p>{repo.description}</p>}
                                    <div className="btn-group">
                                        {enableAPIDocs && (
                                            <Button
                                                to={`${treeOrError.url}/-/docs`}
                                                variant="secondary"
                                                outline={true}
                                                as={Link}
                                            >
                                                <BookOpenBlankVariantIcon className="icon-inline" /> API docs
                                            </Button>
                                        )}
                                        <Button
                                            to={`${treeOrError.url}/-/commits`}
                                            variant="secondary"
                                            outline={true}
                                            as={Link}
                                        >
                                            <SourceCommitIcon className="icon-inline" /> Commits
                                        </Button>
                                        <Button
                                            to={`/${encodeURIPathComponent(repo.name)}/-/branches`}
                                            variant="secondary"
                                            outline={true}
                                            as={Link}
                                        >
                                            <SourceBranchIcon className="icon-inline" /> Branches
                                        </Button>
                                        <Button
                                            to={`/${encodeURIPathComponent(repo.name)}/-/tags`}
                                            variant="secondary"
                                            outline={true}
                                            as={Link}
                                        >
                                            <TagIcon className="icon-inline" /> Tags
                                        </Button>
                                        <Button
                                            to={
                                                revision
                                                    ? `/${encodeURIPathComponent(
                                                          repo.name
                                                      )}/-/compare/...${encodeURIComponent(revision)}`
                                                    : `/${encodeURIPathComponent(repo.name)}/-/compare`
                                            }
                                            variant="secondary"
                                            outline={true}
                                            as={Link}
                                        >
                                            <HistoryIcon className="icon-inline" /> Compare
                                        </Button>
                                        <Button
                                            to={`/${encodeURIPathComponent(repo.name)}/-/stats/contributors`}
                                            variant="secondary"
                                            outline={true}
                                            as={Link}
                                        >
                                            <AccountIcon className="icon-inline" /> Contributors
                                        </Button>
                                        {codeIntelligenceEnabled && (
                                            <Button
                                                to={`/${encodeURIPathComponent(repo.name)}/-/code-intelligence`}
                                                variant="secondary"
                                                outline={true}
                                                as={Link}
                                            >
                                                <BrainIcon className="icon-inline" /> Code Intelligence
                                            </Button>
                                        )}
                                        {batchChangesEnabled && <RepoBatchChangesButton repoName={repo.name} />}
                                        {repo.viewerCanAdminister && (
                                            <Button
                                                to={`/${encodeURIPathComponent(repo.name)}/-/settings`}
                                                variant="secondary"
                                                outline={true}
                                                as={Link}
                                            >
                                                <SettingsIcon className="icon-inline" /> Settings
                                            </Button>
                                        )}
                                    </div>
                                </>
                            ) : (
                                <PageHeader
                                    path={[{ icon: FolderIcon, text: filePath }]}
                                    className="mb-3 test-tree-page-title"
                                />
                            )}
                        </header>

                        <ExtensionViewsSection
                            className={classNames('mb-3', styles.section)}
                            telemetryService={props.telemetryService}
                            settingsCascade={settingsCascade}
                            platformContext={props.platformContext}
                            extensionsController={props.extensionsController}
                            where="directory"
                            uri={uri}
                        />

                        <section className={classNames('test-tree-entries mb-3', styles.section)}>
                            <h2>Files and directories</h2>
                            <TreeEntriesSection
                                parentPath={filePath}
                                entries={treeOrError.entries}
                                fileDecorationsByPath={fileDecorationsByPath}
                                isLightTheme={props.isLightTheme}
                            />
                        </section>
                        <ActionsContainer {...props} menu={ContributableMenu.DirectoryPage} empty={null}>
                            {items => (
                                <section className={styles.section}>
                                    <h2>Actions</h2>
                                    {items.map(item => (
                                        <ActionItem
                                            {...props}
                                            key={item.action.id}
                                            {...item}
                                            className="btn btn-secondary mr-1 mb-1"
                                        />
                                    ))}
                                </section>
                            )}
                        </ActionsContainer>

                        <div className={styles.section}>
                            <h2>Changes</h2>
                            <FilteredConnection<
                                GitCommitFields,
                                Pick<GitCommitNodeProps, 'className' | 'compact' | 'messageSubjectClassName'>
                            >
                                location={props.location}
                                className="mt-2"
                                listClassName="list-group list-group-flush"
                                noun="commit in this tree"
                                pluralNoun="commits in this tree"
                                queryConnection={queryCommits}
                                nodeComponent={GitCommitNode}
                                nodeComponentProps={{
                                    className: classNames('list-group-item', styles.gitCommitNode),
                                    messageSubjectClassName: styles.gitCommitNodeMessageSubject,
                                    compact: true,
                                }}
                                updateOnChange={`${repo.name}:${revision}:${filePath}:${String(showOlderCommits)}`}
                                defaultFirst={7}
                                useURLQuery={false}
                                hideSearch={true}
                                emptyElement={emptyElement}
                                totalCountSummaryComponent={TotalCountSummary}
                            />
                        </div>
                    </>
                )}
            </Container>
        </div>
    )
}
