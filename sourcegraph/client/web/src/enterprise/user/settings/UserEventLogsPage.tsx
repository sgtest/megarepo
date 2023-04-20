import React, { useCallback, useMemo } from 'react'

import classNames from 'classnames'
import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { Timestamp } from '@sourcegraph/branded/src/components/Timestamp'
import { dataOrThrowErrors, gql } from '@sourcegraph/http-client'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Container, PageHeader, Link, Code } from '@sourcegraph/wildcard'

import { requestGraphQL } from '../../../backend/graphql'
import { FilteredConnection } from '../../../components/FilteredConnection'
import { PageTitle } from '../../../components/PageTitle'
import {
    UserEventLogFields,
    UserEventLogsConnectionFields,
    UserEventLogsResult,
    UserEventLogsVariables,
} from '../../../graphql-operations'
import { SiteAdminAlert } from '../../../site-admin/SiteAdminAlert'
import { UserSettingsAreaRouteContext } from '../../../user/settings/UserSettingsArea'

import styles from './UserEventLogsPage.module.scss'

interface UserEventNodeProps {
    /**
     * The user to display in this list item.
     */
    node: UserEventLogFields
}

export const UserEventNode: React.FunctionComponent<React.PropsWithChildren<UserEventNodeProps>> = ({
    node,
}: UserEventNodeProps) => (
    <li className={classNames('list-group-item', styles.eventLog)}>
        <div className="d-flex align-items-center justify-content-between">
            <Code>{node.name}</Code>
            <div>
                <Timestamp date={node.timestamp} />
            </div>
        </div>
        <div className="text-break">
            <small>
                From: {node.source}{' '}
                {node.url && (
                    <span>
                        (<Link to={node.url}>{node.url}</Link>)
                    </span>
                )}
            </small>
        </div>
    </li>
)

export interface UserEventLogsPageProps
    extends Pick<UserSettingsAreaRouteContext, 'authenticatedUser' | 'isSourcegraphDotCom'>,
        UserEventLogsPageContentProps {}

export interface UserEventLogsPageContentProps extends Pick<UserSettingsAreaRouteContext, 'user'>, TelemetryProps {}

/**
 * A page displaying usage statistics for the site.
 */
export const UserEventLogsPage: React.FunctionComponent<React.PropsWithChildren<UserEventLogsPageProps>> = ({
    isSourcegraphDotCom,
    authenticatedUser,
    telemetryService,
    user,
}) => {
    if (isSourcegraphDotCom && authenticatedUser && user.id !== authenticatedUser.id) {
        return (
            <SiteAdminAlert className="sidebar__alert" variant="danger">
                Only the user may access their event logs.
            </SiteAdminAlert>
        )
    }
    return <UserEventLogsPageContent telemetryService={telemetryService} user={user} />
}

export const UserEventLogsPageContent: React.FunctionComponent<
    React.PropsWithChildren<UserEventLogsPageContentProps>
> = ({ telemetryService, user }) => {
    useMemo(() => {
        telemetryService.logViewEvent('UserEventLogPage')
    }, [telemetryService])

    const queryUserEventLogs = useCallback(
        (args: { first?: number }): Observable<UserEventLogsConnectionFields> =>
            requestGraphQL<UserEventLogsResult, UserEventLogsVariables>(
                gql`
                    query UserEventLogs($user: ID!, $first: Int) {
                        node(id: $user) {
                            __typename
                            ... on User {
                                eventLogs(first: $first) {
                                    ...UserEventLogsConnectionFields
                                }
                            }
                        }
                    }

                    fragment UserEventLogsConnectionFields on EventLogsConnection {
                        nodes {
                            ...UserEventLogFields
                        }
                        totalCount
                        pageInfo {
                            hasNextPage
                        }
                    }

                    fragment UserEventLogFields on EventLog {
                        name
                        source
                        url
                        timestamp
                    }
                `,
                { first: args.first ?? null, user: user.id }
            ).pipe(
                map(dataOrThrowErrors),
                map(data => {
                    if (!data.node) {
                        throw new Error('User not found')
                    }
                    if (data.node.__typename !== 'User') {
                        throw new Error(`Requested node is a ${data.node.__typename}, not a User`)
                    }
                    return data.node.eventLogs
                })
            ),
        [user.id]
    )

    return (
        <>
            <PageTitle title="User event log" />
            <PageHeader path={[{ text: 'Event log' }]} headingElement="h2" className="mb-3" />
            <Container className="mb-3">
                <FilteredConnection<UserEventLogFields, {}>
                    key="chronological"
                    defaultFirst={50}
                    className="list-group list-group-flush"
                    hideSearch={true}
                    noun="user event"
                    pluralNoun="user events"
                    queryConnection={queryUserEventLogs}
                    nodeComponent={UserEventNode}
                />
            </Container>
        </>
    )
}
