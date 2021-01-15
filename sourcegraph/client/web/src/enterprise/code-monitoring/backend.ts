import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'
import {
    createInvalidGraphQLMutationResponseError,
    dataOrThrowErrors,
    gql,
} from '../../../../shared/src/graphql/graphql'
import { requestGraphQL } from '../../backend/graphql'
import {
    CreateCodeMonitorResult,
    CreateCodeMonitorVariables,
    DeleteCodeMonitorResult,
    DeleteCodeMonitorVariables,
    FetchCodeMonitorResult,
    FetchCodeMonitorVariables,
    ListCodeMonitors,
    ListUserCodeMonitorsResult,
    ListUserCodeMonitorsVariables,
    MonitorEditActionInput,
    MonitorEditInput,
    MonitorEditTriggerInput,
    ResetTriggerQueryTimestampsResult,
    ResetTriggerQueryTimestampsVariables,
    Scalars,
    ToggleCodeMonitorEnabledResult,
    ToggleCodeMonitorEnabledVariables,
    UpdateCodeMonitorResult,
    UpdateCodeMonitorVariables,
    TriggerTestEmailActionResult,
    TriggerTestEmailActionVariables,
} from '../../graphql-operations'

const CodeMonitorFragment = gql`
    fragment CodeMonitorFields on Monitor {
        id
        description
        enabled
        trigger {
            ... on MonitorQuery {
                id
                query
            }
        }
        actions {
            nodes {
                ... on MonitorEmail {
                    id
                    enabled
                    recipients {
                        nodes {
                            id
                        }
                    }
                }
            }
        }
    }
`

const ListCodeMonitorsFragment = gql`
    fragment ListCodeMonitors on MonitorConnection {
        nodes {
            ...CodeMonitorFields
        }
        totalCount
        pageInfo {
            endCursor
            hasNextPage
        }
    }
    ${CodeMonitorFragment}
`

export const createCodeMonitor = ({
    monitor,
    trigger,
    actions,
}: CreateCodeMonitorVariables): Observable<CreateCodeMonitorResult['createCodeMonitor']> => {
    const query = gql`
        mutation CreateCodeMonitor(
            $monitor: MonitorInput!
            $trigger: MonitorTriggerInput!
            $actions: [MonitorActionInput!]!
        ) {
            createCodeMonitor(monitor: $monitor, trigger: $trigger, actions: $actions) {
                description
            }
        }
    `

    return requestGraphQL<CreateCodeMonitorResult, CreateCodeMonitorVariables>(query, {
        monitor,
        trigger,
        actions,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => data.createCodeMonitor)
    )
}

export const fetchUserCodeMonitors = ({
    id,
    first,
    after,
}: ListUserCodeMonitorsVariables): Observable<ListCodeMonitors> => {
    const query = gql`
        query ListUserCodeMonitors($id: ID!, $first: Int, $after: String) {
            node(id: $id) {
                __typename
                ... on User {
                    monitors(first: $first, after: $after) {
                        ...ListCodeMonitors
                    }
                }
            }
        }
        ${ListCodeMonitorsFragment}
    `

    return requestGraphQL<ListUserCodeMonitorsResult, ListUserCodeMonitorsVariables>(query, {
        id,
        first,
        after,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.node) {
                throw new Error('namespace not found')
            }

            if (data.node.__typename !== 'User') {
                throw new Error(`Requested node is a ${data.node.__typename}, not a User or Org`)
            }

            return data.node.monitors
        })
    )
}

export const toggleCodeMonitorEnabled = (
    id: string,
    enabled: boolean
): Observable<ToggleCodeMonitorEnabledResult['toggleCodeMonitor']> => {
    const query = gql`
        mutation ToggleCodeMonitorEnabled($id: ID!, $enabled: Boolean!) {
            toggleCodeMonitor(id: $id, enabled: $enabled) {
                id
                enabled
            }
        }
    `

    return requestGraphQL<ToggleCodeMonitorEnabledResult, ToggleCodeMonitorEnabledVariables>(query, {
        id,
        enabled,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => data.toggleCodeMonitor)
    )
}

export const fetchCodeMonitor = (id: string): Observable<FetchCodeMonitorResult> => {
    const query = gql`
        query FetchCodeMonitor($id: ID!) {
            node(id: $id) {
                ... on Monitor {
                    id
                    description
                    owner {
                        id
                        namespaceName
                    }
                    enabled
                    actions {
                        nodes {
                            ... on MonitorEmail {
                                id
                                recipients {
                                    nodes {
                                        id
                                        url
                                    }
                                }
                                enabled
                            }
                        }
                    }
                    trigger {
                        ... on MonitorQuery {
                            id
                            query
                        }
                    }
                }
            }
        }
    `

    return requestGraphQL<FetchCodeMonitorResult, FetchCodeMonitorVariables>(query, {
        id,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => data)
    )
}

export const updateCodeMonitor = (
    monitorEditInput: MonitorEditInput,
    triggerEditInput: MonitorEditTriggerInput,
    actionEditInput: MonitorEditActionInput[]
): Observable<UpdateCodeMonitorResult['updateCodeMonitor']> => {
    const updateCodeMonitorQuery = gql`
        mutation UpdateCodeMonitor(
            $monitor: MonitorEditInput!
            $trigger: MonitorEditTriggerInput!
            $actions: [MonitorEditActionInput!]!
        ) {
            updateCodeMonitor(monitor: $monitor, trigger: $trigger, actions: $actions) {
                ...CodeMonitorFields
            }
        }
        ${CodeMonitorFragment}
    `

    return requestGraphQL<UpdateCodeMonitorResult, UpdateCodeMonitorVariables>(updateCodeMonitorQuery, {
        monitor: monitorEditInput,
        trigger: triggerEditInput,
        actions: actionEditInput,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => data.updateCodeMonitor)
    )
}

export const deleteCodeMonitor = (id: Scalars['ID']): Observable<void> => {
    const deleteCodeMonitorQuery = gql`
        mutation DeleteCodeMonitor($id: ID!) {
            deleteCodeMonitor(id: $id) {
                alwaysNil
            }
        }
    `

    return requestGraphQL<DeleteCodeMonitorResult, DeleteCodeMonitorVariables>(deleteCodeMonitorQuery, { id }).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.deleteCodeMonitor) {
                throw createInvalidGraphQLMutationResponseError('DeleteCodeMonitor')
            }
        })
    )
}

export const sendTestEmail = (id: Scalars['ID']): Observable<void> => {
    const query = gql`
        mutation ResetTriggerQueryTimestamps($id: ID!) {
            resetTriggerQueryTimestamps(id: $id) {
                alwaysNil
            }
        }
    `

    return requestGraphQL<ResetTriggerQueryTimestampsResult, ResetTriggerQueryTimestampsVariables>(query, { id }).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.resetTriggerQueryTimestamps) {
                console.log('DATA', data)
                throw createInvalidGraphQLMutationResponseError('ResetTriggerQueryTimestamps')
            }
        })
    )
}

export const triggerTestEmailAction = ({
    namespace,
    description,
    email,
}: TriggerTestEmailActionVariables): Observable<void> => {
    const query = gql`
        mutation TriggerTestEmailAction($namespace: ID!, $description: String!, $email: MonitorEmailInput!) {
            triggerTestEmailAction(namespace: $namespace, description: $description, email: $email) {
                alwaysNil
            }
        }
    `

    return requestGraphQL<TriggerTestEmailActionResult, TriggerTestEmailActionVariables>(query, {
        namespace,
        description,
        email,
    }).pipe(
        map(dataOrThrowErrors),
        map(data => {
            if (!data.triggerTestEmailAction) {
                throw createInvalidGraphQLMutationResponseError('TriggerTestEmailAction')
            }
        })
    )
}
