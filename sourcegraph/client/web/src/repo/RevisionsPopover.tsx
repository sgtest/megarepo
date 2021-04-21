import { Tab, TabList, TabPanel, TabPanels, Tabs } from '@reach/tabs'
import * as H from 'history'
import React, { useCallback, useEffect } from 'react'
import { Link } from 'react-router-dom'
import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { CircleChevronLeftIcon } from '@sourcegraph/shared/src/components/icons'
import { GitRefType, Scalars } from '@sourcegraph/shared/src/graphql-operations'
import { gql, dataOrThrowErrors } from '@sourcegraph/shared/src/graphql/graphql'
import { memoizeObservable } from '@sourcegraph/shared/src/util/memoizeObservable'
import { RevisionSpec } from '@sourcegraph/shared/src/util/url'
import { useLocalStorage } from '@sourcegraph/shared/src/util/useLocalStorage'

import { requestGraphQL } from '../backend/graphql'
import { FilteredConnection, FilteredConnectionQueryArguments } from '../components/FilteredConnection'
import {
    GitCommitAncestorFields,
    GitCommitAncestorsConnectionFields,
    GitRefConnectionFields,
    GitRefFields,
    RepositoryGitCommitResult,
    RepositoryGitCommitVariables,
} from '../graphql-operations'
import { eventLogger } from '../tracking/eventLogger'
import { replaceRevisionInURL } from '../util/url'

import { GitReferenceNode, queryGitReferences } from './GitReference'

const fetchRepositoryCommits = memoizeObservable(
    (
        args: RevisionSpec & { repo: Scalars['ID']; first?: number; query?: string }
    ): Observable<GitCommitAncestorsConnectionFields> =>
        requestGraphQL<RepositoryGitCommitResult, RepositoryGitCommitVariables>(
            gql`
                query RepositoryGitCommit($repo: ID!, $first: Int, $revision: String!, $query: String) {
                    node(id: $repo) {
                        __typename
                        ... on Repository {
                            commit(rev: $revision) {
                                ancestors(first: $first, query: $query) {
                                    ...GitCommitAncestorsConnectionFields
                                }
                            }
                        }
                    }
                }

                fragment GitCommitAncestorsConnectionFields on GitCommitConnection {
                    nodes {
                        ...GitCommitAncestorFields
                    }
                    pageInfo {
                        hasNextPage
                    }
                }

                fragment GitCommitAncestorFields on GitCommit {
                    id
                    oid
                    abbreviatedOID
                    author {
                        person {
                            name
                            avatarURL
                        }
                        date
                    }
                    subject
                }
            `,
            {
                first: args.first ?? null,
                query: args.query ?? null,
                repo: args.repo,
                revision: args.revision,
            }
        ).pipe(
            map(dataOrThrowErrors),
            map(({ node }) => {
                if (!node) {
                    throw new Error(`Repository ${args.repo} not found`)
                }
                if (node.__typename !== 'Repository') {
                    throw new Error(`Node is a ${node.__typename}, not a Repository`)
                }
                if (!node.commit?.ancestors) {
                    throw new Error(`Cannot load ancestors for repository ${args.repo}`)
                }
                return node.commit.ancestors
            })
        ),
    args => JSON.stringify(args)
)

interface GitReferencePopoverNodeProps {
    node: GitRefFields

    defaultBranch: string
    currentRevision: string | undefined

    location: H.Location
}

const GitReferencePopoverNode: React.FunctionComponent<GitReferencePopoverNodeProps> = ({
    node,
    defaultBranch,
    currentRevision,
    location,
}) => {
    let isCurrent: boolean
    if (currentRevision) {
        isCurrent = node.name === currentRevision || node.abbrevName === currentRevision
    } else {
        isCurrent = node.name === `refs/heads/${defaultBranch}`
    }
    return (
        <GitReferenceNode
            node={node}
            url={replaceRevisionInURL(location.pathname + location.search + location.hash, node.abbrevName)}
            ancestorIsLink={false}
        >
            {isCurrent && (
                <CircleChevronLeftIcon
                    className="icon-inline connection-popover__node-link-icon"
                    data-tooltip="Current"
                />
            )}
        </GitReferenceNode>
    )
}

interface GitCommitNodeProps {
    node: GitCommitAncestorFields

    currentCommitID: string | undefined

    location: H.Location
}

const GitCommitNode: React.FunctionComponent<GitCommitNodeProps> = ({ node, currentCommitID, location }) => {
    const isCurrent = currentCommitID === node.oid
    return (
        <li key={node.oid} className="connection-popover__node revisions-popover-git-commit-node">
            <Link
                to={replaceRevisionInURL(location.pathname + location.search + location.hash, node.oid)}
                className={`connection-popover__node-link ${
                    isCurrent ? 'connection-popover__node-link--active' : ''
                } revisions-popover-git-commit-node__link`}
            >
                <code className="revisions-popover-git-commit-node__oid" title={node.oid}>
                    {node.abbreviatedOID}
                </code>
                <span className="revisions-popover-git-commit-node__message">{node.subject.slice(0, 200)}</span>
                {isCurrent && (
                    <CircleChevronLeftIcon
                        className="icon-inline connection-popover__node-link-icon"
                        data-tooltip="Current commit"
                    />
                )}
            </Link>
        </li>
    )
}

interface Props {
    repo: Scalars['ID']
    repoName: string
    defaultBranch: string

    /** The current revision, or undefined for the default branch. */
    currentRev: string | undefined

    currentCommitID?: string

    history: H.History
    location: H.Location
}

type RevisionsPopoverTabID = 'branches' | 'tags' | 'commits'

interface RevisionsPopoverTab {
    id: RevisionsPopoverTabID
    label: string
    noun: string
    pluralNoun: string
    type?: GitRefType
}

const LAST_TAB_STORAGE_KEY = 'RevisionsPopover.lastTab'

const TABS: RevisionsPopoverTab[] = [
    { id: 'branches', label: 'Branches', noun: 'branch', pluralNoun: 'branches', type: GitRefType.GIT_BRANCH },
    { id: 'tags', label: 'Tags', noun: 'tag', pluralNoun: 'tags', type: GitRefType.GIT_TAG },
    { id: 'commits', label: 'Commits', noun: 'commit', pluralNoun: 'commits' },
]

/**
 * A popover that displays a searchable list of revisions (grouped by type) for
 * the current repository.
 */
export const RevisionsPopover: React.FunctionComponent<Props> = props => {
    useEffect(() => {
        eventLogger.logViewEvent('RevisionsPopover')
    }, [])

    const [tabIndex, setTabIndex] = useLocalStorage(LAST_TAB_STORAGE_KEY, 0)
    const handleTabsChange = useCallback((index: number) => setTabIndex(index), [setTabIndex])

    const queryGitBranches = (args: FilteredConnectionQueryArguments): Observable<GitRefConnectionFields> =>
        queryGitReferences({ ...args, repo: props.repo, type: GitRefType.GIT_BRANCH, withBehindAhead: false })

    const queryGitTags = (args: FilteredConnectionQueryArguments): Observable<GitRefConnectionFields> =>
        queryGitReferences({ ...args, repo: props.repo, type: GitRefType.GIT_TAG, withBehindAhead: false })

    const queryRepositoryCommits = (
        args: FilteredConnectionQueryArguments
    ): Observable<GitCommitAncestorsConnectionFields> =>
        fetchRepositoryCommits({
            ...args,
            repo: props.repo,
            revision: props.currentRev || props.defaultBranch,
        })

    return (
        <Tabs defaultIndex={tabIndex} className="revisions-popover" onChange={handleTabsChange}>
            <div className="tablist-wrapper w-100 align-items-center">
                <TabList>
                    {TABS.map(({ label, id }) => (
                        <Tab className="d-flex flex-1 justify-content-around" key={id} data-test-tab={id}>
                            {label}
                        </Tab>
                    ))}
                </TabList>
            </div>
            <TabPanels className="revisions-popover__tabs">
                {TABS.map(tab => (
                    <TabPanel className="" key={tab.id}>
                        {tab.type ? (
                            <FilteredConnection<GitRefFields, Omit<GitReferencePopoverNodeProps, 'node'>>
                                key={tab.id}
                                className="connection-popover__content"
                                showMoreClassName="connection-popover__show-more"
                                compact={true}
                                noun={tab.noun}
                                pluralNoun={tab.pluralNoun}
                                queryConnection={tab.type === GitRefType.GIT_BRANCH ? queryGitBranches : queryGitTags}
                                nodeComponent={GitReferencePopoverNode}
                                nodeComponentProps={{
                                    defaultBranch: props.defaultBranch,
                                    currentRevision: props.currentRev,
                                    location: props.location,
                                }}
                                defaultFirst={50}
                                autoFocus={true}
                                noSummaryIfAllNodesVisible={true}
                                useURLQuery={false}
                                history={props.history}
                                location={props.location}
                            />
                        ) : (
                            <FilteredConnection<GitCommitAncestorFields, Omit<GitCommitNodeProps, 'node'>>
                                key={tab.id}
                                className="connection-popover__content"
                                compact={true}
                                noun={tab.noun}
                                pluralNoun={tab.pluralNoun}
                                // eslint-disable-next-line react/jsx-no-bind
                                queryConnection={queryRepositoryCommits}
                                nodeComponent={GitCommitNode}
                                nodeComponentProps={{
                                    currentCommitID: props.currentCommitID,
                                    location: props.location,
                                }}
                                defaultFirst={15}
                                autoFocus={true}
                                history={props.history}
                                location={props.location}
                                noSummaryIfAllNodesVisible={true}
                                useURLQuery={false}
                            />
                        )}
                    </TabPanel>
                ))}
            </TabPanels>
        </Tabs>
    )
}
