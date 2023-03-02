import { MutationTuple, QueryResult } from '@apollo/client'

import { dataOrThrowErrors, gql, useMutation, useQuery } from '@sourcegraph/http-client'

import {
    useShowMorePagination,
    UseShowMorePaginationResult,
} from '../../components/FilteredConnection/hooks/useShowMorePagination'
import {
    DeleteRoleVariables,
    DeleteRoleResult,
    AllRolesVariables,
    AllRolesResult,
    RoleFields,
    CreateRoleResult,
    CreateRoleVariables,
    AllPermissionsResult,
    AllPermissionsVariables,
    PermissionNamespace,
    PermissionFields,
} from '../../graphql-operations'

export const DEFAULT_PAGE_LIMIT = 10

const permissionFragment = gql`
    fragment PermissionFields on Permission {
        __typename
        id
        namespace
        action
        displayName
    }
`

const roleFragment = gql`
    fragment RoleFields on Role {
        __typename
        id
        name
        system
        permissions {
            nodes {
                ...PermissionFields
            }
        }
    }

    ${permissionFragment}
`

export const ROLES_QUERY = gql`
    query AllRoles($first: Int, $after: String) {
        roles(first: $first, after: $after) {
            __typename
            totalCount
            pageInfo {
                hasNextPage
                endCursor
            }
            nodes {
                ...RoleFields
            }
        }
    }

    ${roleFragment}
`

export const CREATE_ROLE = gql`
    mutation CreateRole($name: String!, $permissions: [ID!]!) {
        createRole(name: $name, permissions: $permissions) {
            ...RoleFields
        }
    }

    ${roleFragment}
`

export const DELETE_ROLE = gql`
    mutation DeleteRole($role: ID!) {
        deleteRole(role: $role) {
            alwaysNil
        }
    }
`

export const ALL_PERMISSIONS = gql`
    query AllPermissions {
        permissions {
            nodes {
                ...PermissionFields
            }
        }
    }

    ${permissionFragment}
`

export const useRolesConnection = (): UseShowMorePaginationResult<AllRolesResult, RoleFields> =>
    useShowMorePagination<AllRolesResult, AllRolesVariables, RoleFields>({
        query: ROLES_QUERY,
        variables: {
            first: DEFAULT_PAGE_LIMIT,
            after: null,
        },
        options: {
            fetchPolicy: 'no-cache',
        },
        getConnection: result => {
            const { roles } = dataOrThrowErrors(result)
            return roles
        },
    })

export const usePermissions = (
    onCompleted: (result: AllPermissionsResult) => void
): QueryResult<AllPermissionsResult, AllPermissionsVariables> =>
    useQuery<AllPermissionsResult, AllPermissionsVariables>(ALL_PERMISSIONS, {
        fetchPolicy: 'cache-and-network',
        onCompleted,
    })

export const useCreateRole = (): MutationTuple<CreateRoleResult, CreateRoleVariables> => useMutation(CREATE_ROLE)

export const useDeleteRole = (
    onCompleted: () => void,
    onError: () => void
): MutationTuple<DeleteRoleResult, DeleteRoleVariables> => useMutation(DELETE_ROLE, { onCompleted, onError })

export type PermissionsMap = Record<PermissionNamespace, PermissionFields[]>

// Permissions are grouped by their namespace in the UI. We do this to get all unique namespaces
// on the Sourcegraph instance.
export const allNamespaces = Object.values<PermissionNamespace>(PermissionNamespace)
