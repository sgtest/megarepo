import * as React from 'react'
import { useEffect, useState } from 'react'

import classNames from 'classnames'
import { escapeRegExp } from 'lodash'
import { RouteComponentProps } from 'react-router-dom'

import { Form } from '@sourcegraph/branded/src/components/Form'
import { numberWithCommas, pluralize } from '@sourcegraph/common'
import { gql, dataOrThrowErrors } from '@sourcegraph/http-client'
import { SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'
import * as GQL from '@sourcegraph/shared/src/schema'
import { buildSearchURLQuery } from '@sourcegraph/shared/src/util/url'
import { Button, ButtonGroup, Link, CardHeader, CardBody, Card, Input, Label, Tooltip } from '@sourcegraph/wildcard'

import { useConnection } from '../../components/FilteredConnection/hooks/useConnection'
import {
    ConnectionList,
    ConnectionContainer,
    ConnectionLoading,
    ConnectionError,
    SummaryContainer,
    ConnectionSummary,
    ShowMoreButton,
} from '../../components/FilteredConnection/ui'
import { PageTitle } from '../../components/PageTitle'
import { Timestamp } from '../../components/time/Timestamp'
import {
    RepositoryContributorNodeFields,
    RepositoryContributorsResult,
    RepositoryContributorsVariables,
} from '../../graphql-operations'
import { PersonLink } from '../../person/PersonLink'
import { quoteIfNeeded, searchQueryForRepoRevision } from '../../search'
import { eventLogger } from '../../tracking/eventLogger'
import { UserAvatar } from '../../user/UserAvatar'

import { RepositoryStatsAreaPageProps } from './RepositoryStatsArea'

import styles from './RepositoryStatsContributorsPage.module.scss'

interface QuerySpec {
    revisionRange: string
    after: string
    path: string
}

interface RepositoryContributorNodeProps extends QuerySpec {
    node: RepositoryContributorNodeFields
    repoName: string
    globbing: boolean
}

const RepositoryContributorNode: React.FunctionComponent<React.PropsWithChildren<RepositoryContributorNodeProps>> = ({
    node,
    repoName,
    revisionRange,
    after,
    path,
    globbing,
}) => {
    const commit = node.commits.nodes[0] as GQL.IGitCommit | undefined

    const query: string = [
        searchQueryForRepoRevision(repoName, globbing),
        'type:diff',
        `author:${quoteIfNeeded(node.person.email)}`,
        after ? `after:${quoteIfNeeded(after)}` : '',
        path ? `file:${quoteIfNeeded(escapeRegExp(path))}` : '',
    ]
        .join(' ')
        .replace(/\s+/, ' ')

    return (
        <li className={classNames('list-group-item py-2', styles.repositoryContributorNode)}>
            <div className={styles.person}>
                <UserAvatar inline={true} className="mr-2" user={node.person} />
                <PersonLink userClassName="font-weight-bold" person={node.person} />
            </div>
            <div className={styles.commits}>
                <div className={styles.commit}>
                    {commit && (
                        <>
                            <Timestamp date={commit.author.date} />:{' '}
                            <Tooltip content="Most recent commit by contributor" placement="bottom">
                                <Link to={commit.url} className="repository-contributor-node__commit-subject">
                                    {commit.subject}
                                </Link>
                            </Tooltip>
                        </>
                    )}
                </div>
                <div className={styles.count}>
                    <Tooltip
                        content={
                            revisionRange?.includes('..')
                                ? 'All commits will be shown (revision end ranges are not yet supported)'
                                : null
                        }
                        placement="left"
                    >
                        <Link
                            to={`/search?${buildSearchURLQuery(query, SearchPatternType.standard, false)}`}
                            className="font-weight-bold"
                        >
                            {numberWithCommas(node.count)} {pluralize('commit', node.count)}
                        </Link>
                    </Tooltip>
                </div>
            </div>
        </li>
    )
}

const CONTRIBUTORS_QUERY = gql`
    query RepositoryContributors($repo: ID!, $first: Int, $revisionRange: String, $afterDate: String, $path: String) {
        node(id: $repo) {
            ... on Repository {
                contributors(first: $first, revisionRange: $revisionRange, afterDate: $afterDate, path: $path) {
                    ...RepositoryContributorConnectionFields
                }
            }
        }
    }

    fragment RepositoryContributorConnectionFields on RepositoryContributorConnection {
        totalCount
        pageInfo {
            hasNextPage
        }
        nodes {
            ...RepositoryContributorNodeFields
        }
    }

    fragment RepositoryContributorNodeFields on RepositoryContributor {
        person {
            name
            displayName
            email
            avatarURL
            user {
                username
                url
                displayName
            }
        }
        count
        commits(first: 1) {
            nodes {
                oid
                abbreviatedOID
                url
                subject
                author {
                    date
                }
            }
        }
    }
`

const BATCH_COUNT = 20

const equalOrEmpty = (a: string | undefined, b: string | undefined): boolean => a === b || (!a && !b)

interface Props extends RepositoryStatsAreaPageProps, RouteComponentProps<{}> {
    globbing: boolean
}

const contributorsPageInputIds: Record<string, string> = {
    REVISION_RANGE: 'repository-stats-contributors-page__revision-range',
    AFTER: 'repository-stats-contributors-page__after',
    PATH: 'repository-stats-contributors-page__path',
}

// Get query params from spec
const getUrlQuery = (spec: Partial<QuerySpec>): string => {
    const search = new URLSearchParams()
    for (const [key, value] of Object.entries(spec)) {
        if (value) {
            search.set(key, value)
        }
    }
    return search.toString()
}

/** A page that shows a repository's contributors. */
export const RepositoryStatsContributorsPage: React.FunctionComponent<Props> = ({
    location,
    history,
    repo,
    globbing,
}) => {
    const queryParameters = new URLSearchParams(location.search)
    const spec: QuerySpec = {
        revisionRange: queryParameters.get('revisionRange') ?? '',
        after: queryParameters.get('after') ?? '',
        path: queryParameters.get('path') ?? '',
    }

    const [revisionRange, setRevisionRange] = useState(spec.revisionRange)
    const [after, setAfter] = useState(spec.after)
    const [path, setPath] = useState(spec.path)

    const { connection, error, loading, hasNextPage, fetchMore } = useConnection<
        RepositoryContributorsResult,
        RepositoryContributorsVariables,
        RepositoryContributorNodeFields
    >({
        query: CONTRIBUTORS_QUERY,
        variables: {
            first: BATCH_COUNT,
            repo: repo.id,
            revisionRange: spec.revisionRange,
            afterDate: spec.after,
            path: spec.path,
        },
        getConnection: result => {
            const { node } = dataOrThrowErrors(result)
            if (!node) {
                throw new Error(`Node ${repo.id} not found`)
            }
            if (!('contributors' in node)) {
                throw new Error('Failed to fetch contributors for this repo')
            }
            return node.contributors
        },
        options: {
            fetchPolicy: 'cache-first',
        },
    })

    // Log page view when initially rendered
    useEffect(() => {
        eventLogger.logPageView('RepositoryStatsContributors')
    }, [])

    // Update spec when search params change
    useEffect(() => {
        setRevisionRange(spec.revisionRange)
        setAfter(spec.after)
        setPath(spec.path)
        // We only want to run this effect when `location.search` is updated.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [location.search])

    // Update the buffer values, but don't update the URL
    const onChange: React.ChangeEventHandler<HTMLInputElement> = event => {
        const { value } = event.target
        switch (event.currentTarget.id) {
            case contributorsPageInputIds.REVISION_RANGE:
                setRevisionRange(value)
                break
            case contributorsPageInputIds.AFTER:
                setAfter(value)
                break
            case contributorsPageInputIds.PATH:
                setPath(value)
                break
        }
    }

    // Update the URL to reflect buffer state
    const onSubmit: React.FormEventHandler<HTMLFormElement> = event => {
        event.preventDefault()
        history.push({
            search: getUrlQuery({ revisionRange, after, path }),
        })
    }

    // Reset the buffer state to the original state
    const onCancel: React.MouseEventHandler<HTMLButtonElement> = event => {
        event.preventDefault()
        setRevisionRange(spec.revisionRange)
        setAfter(spec.after)
        setPath(spec.path)
    }

    // Push new query param to history, state change will follow via `useEffect` on `location.search`
    const updateAfter = (after: string | undefined): void => {
        history.push({ search: getUrlQuery({ ...spec, after }) })
    }

    // Whether the user has entered new option values that differ from what's in the URL query and has not yet
    // submitted the form.
    const stateDiffers =
        !equalOrEmpty(spec.revisionRange, revisionRange) ||
        !equalOrEmpty(spec.after, after) ||
        !equalOrEmpty(spec.path, path)

    const Contributors: React.FunctionComponent = () => (
        <ConnectionContainer>
            {error && <ConnectionError errors={[error.message]} />}
            {connection && connection.nodes.length > 0 && (
                <ConnectionList className="list-group list-group-flush test-filtered-contributors-connection">
                    {connection.nodes.map(node => (
                        <RepositoryContributorNode
                            key={`${node.person.displayName}:${node.count}`}
                            node={node}
                            repoName={repo.name}
                            globbing={globbing}
                            {...spec}
                        />
                    ))}
                </ConnectionList>
            )}
            {loading && <ConnectionLoading />}
            <SummaryContainer>
                {connection && (
                    <ConnectionSummary
                        connection={connection}
                        first={BATCH_COUNT}
                        noun="contributor"
                        pluralNoun="contributors"
                        hasNextPage={hasNextPage}
                    />
                )}
                {hasNextPage && <ShowMoreButton onClick={fetchMore} />}
            </SummaryContainer>
        </ConnectionContainer>
    )

    return (
        <section>
            <PageTitle title="Contributors" />
            <Card className={styles.card}>
                <CardHeader as="header">Contributions filter</CardHeader>
                <CardBody>
                    <Form onSubmit={onSubmit}>
                        <div className={classNames(styles.row, 'form-inline')}>
                            <div className="input-group mb-2 mr-sm-2">
                                <div className="input-group-prepend">
                                    <Label htmlFor={contributorsPageInputIds.AFTER} className="input-group-text">
                                        Time period
                                    </Label>
                                </div>
                                <Input
                                    name="after"
                                    size={12}
                                    id={contributorsPageInputIds.AFTER}
                                    value={after || ''}
                                    placeholder="All time"
                                    onChange={onChange}
                                />
                                <div className="input-group-append">
                                    <ButtonGroup aria-label="Time period presets">
                                        <Button
                                            className={classNames(
                                                styles.btnNoLeftRoundedCorners,
                                                spec.after === '7 days ago' && 'active'
                                            )}
                                            onClick={() => updateAfter('7 days ago')}
                                            variant="secondary"
                                        >
                                            Last 7 days
                                        </Button>
                                        <Button
                                            className={classNames(spec.after === '30 days ago' && 'active')}
                                            onClick={() => updateAfter('30 days ago')}
                                            variant="secondary"
                                        >
                                            Last 30 days
                                        </Button>
                                        <Button
                                            className={classNames(spec.after === '1 year ago' && 'active')}
                                            onClick={() => updateAfter('1 year ago')}
                                            variant="secondary"
                                        >
                                            Last year
                                        </Button>
                                        <Button
                                            className={classNames(!spec.after && 'active')}
                                            onClick={() => updateAfter(undefined)}
                                            variant="secondary"
                                        >
                                            All time
                                        </Button>
                                    </ButtonGroup>
                                </div>
                            </div>
                        </div>
                        <div className={classNames(styles.row, 'form-inline')}>
                            <div className="input-group mt-2 mr-sm-2">
                                <div className="input-group-prepend">
                                    <Label
                                        htmlFor={contributorsPageInputIds.REVISION_RANGE}
                                        className="input-group-text"
                                    >
                                        Revision range
                                    </Label>
                                </div>
                                <Input
                                    name="revision-range"
                                    size={18}
                                    id={contributorsPageInputIds.REVISION_RANGE}
                                    value={revisionRange || ''}
                                    placeholder="Default branch"
                                    onChange={onChange}
                                    autoCapitalize="off"
                                    autoCorrect="off"
                                    autoComplete="off"
                                    spellCheck={false}
                                />
                            </div>
                            <div className="input-group mt-2 mr-sm-2">
                                <div className="input-group-prepend">
                                    <Label htmlFor={contributorsPageInputIds.PATH} className="input-group-text">
                                        Path
                                    </Label>
                                </div>
                                <Input
                                    name="path"
                                    size={18}
                                    id={contributorsPageInputIds.PATH}
                                    value={path || ''}
                                    placeholder="All files"
                                    onChange={onChange}
                                    autoCapitalize="off"
                                    autoCorrect="off"
                                    autoComplete="off"
                                    spellCheck={false}
                                />
                            </div>
                            {stateDiffers && (
                                <div className="form-group mb-0">
                                    <Button type="submit" className="mr-2 mt-2" variant="primary">
                                        Update
                                    </Button>
                                    <Button type="reset" className="mt-2" onClick={onCancel} variant="secondary">
                                        Cancel
                                    </Button>
                                </div>
                            )}
                        </div>
                    </Form>
                </CardBody>
            </Card>
            <Contributors />
        </section>
    )
}
