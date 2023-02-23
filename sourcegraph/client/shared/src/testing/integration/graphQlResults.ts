import { AuthenticatedUser } from '../../auth'
import { SharedGraphQlOperations } from '../../graphql-operations'

export const testUserID = 'TestUserID'
export const settingsID = 123

export const currentUserMock = {
    __typename: 'User',
    id: testUserID,
    databaseID: 1,
    username: 'test',
    avatarURL: null,
    displayName: null,
    siteAdmin: true,
    tags: [],
    tosAccepted: true,
    url: '/users/test',
    settingsURL: '/users/test/settings',
    organizations: { nodes: [] },
    session: { canSignOut: true },
    viewerCanAdminister: true,
    searchable: true,
    emails: [{ email: 'felix@sourcegraph.com', isPrimary: true, verified: true }],
    latestSettings: null,
} satisfies AuthenticatedUser

/**
 * Predefined results for GraphQL requests that are made on almost every page.
 */
export const sharedGraphQlResults: Partial<SharedGraphQlOperations> = {}

export const emptyResponse = {
    alwaysNil: null,
}
