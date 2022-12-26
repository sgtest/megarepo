import { SharedGraphQlOperations } from '@sourcegraph/shared/src/graphql-operations'
import { Settings } from '@sourcegraph/shared/src/schema/settings.schema'
import { mergeSettings } from '@sourcegraph/shared/src/settings/settings'
import { testUserID, sharedGraphQlResults } from '@sourcegraph/shared/src/testing/integration/graphQlResults'

import { WebGraphQlOperations } from '../graphql-operations'
import {
    collaboratorsPayload,
    recentFilesPayload,
    recentSearchesPayload,
    savedSearchesPayload,
} from '../search/panels/utils'

import { builtinAuthProvider, siteGQLID, siteID } from './jscontext'

/**
 * Helper function for creating user and organization/site settings.
 */
export const createViewerSettingsGraphQLOverride = (
    settings: { user?: Settings; site?: Settings } = {}
): Pick<SharedGraphQlOperations, 'ViewerSettings'> => {
    const { user: userSettings = {}, site: siteSettings = {} } = settings
    return {
        ViewerSettings: () => ({
            viewerSettings: {
                __typename: 'SettingsCascade',
                subjects: [
                    {
                        __typename: 'DefaultSettings',
                        id: 'TestDefaultSettingsID',
                        settingsURL: null,
                        viewerCanAdminister: false,
                        latestSettings: {
                            id: 0,
                            contents: JSON.stringify(userSettings),
                        },
                    },
                    {
                        __typename: 'Site',
                        id: siteGQLID,
                        siteID,
                        latestSettings: {
                            id: 470,
                            contents: JSON.stringify(siteSettings),
                        },
                        settingsURL: '/site-admin/global-settings',
                        viewerCanAdminister: true,
                        allowSiteSettingsEdits: true,
                    },
                ],
                final: JSON.stringify(mergeSettings([siteSettings, userSettings])),
            },
        }),
    }
}

/**
 * Predefined results for GraphQL requests that are made on almost every page.
 */
export const commonWebGraphQlResults: Partial<WebGraphQlOperations & SharedGraphQlOperations> = {
    ...sharedGraphQlResults,
    CurrentAuthState: () => ({
        currentUser: {
            __typename: 'User',
            id: testUserID,
            databaseID: 1,
            username: 'test',
            avatarURL: null,
            email: 'felix@sourcegraph.com',
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
            emails: [],
            latestSettings: null,
        },
    }),
    ...createViewerSettingsGraphQLOverride(),
    SiteFlags: () => ({
        site: {
            needsRepositoryConfiguration: false,
            freeUsersExceeded: false,
            alerts: [],
            authProviders: {
                nodes: [builtinAuthProvider],
            },
            disableBuiltInSearches: false,
            sendsEmailVerificationEmails: true,
            updateCheck: {
                pending: false,
                checkedAt: '2020-07-07T12:31:16+02:00',
                errorMessage: null,
                updateVersionAvailable: null,
            },
            productSubscription: {
                license: { expiresAt: '3021-05-28T16:06:40Z' },
                noLicenseWarningUserCount: null,
            },
            productVersion: '0.0.0+dev',
        },
        productVersion: '0.0.0+dev',
    }),

    StatusMessages: () => ({
        statusMessages: [],
    }),

    EventLogsData: () => ({
        node: {
            __typename: 'User',
            eventLogs: {
                nodes: [],
                totalCount: 0,
                pageInfo: {
                    hasNextPage: false,
                    endCursor: null,
                },
            },
        },
    }),
    savedSearches: () => ({
        savedSearches: [],
    }),
    LogEvents: () => ({
        logEvents: {
            alwaysNil: null,
        },
    }),
    ListSearchContexts: () => ({
        searchContexts: {
            nodes: [],
            totalCount: 0,
            pageInfo: { hasNextPage: false, endCursor: null },
        },
    }),
    IsSearchContextAvailable: () => ({
        isSearchContextAvailable: false,
    }),
    ExternalServices: () => ({
        externalServices: {
            totalCount: 0,
            nodes: [],
            pageInfo: { hasNextPage: false, endCursor: null },
        },
    }),
    EvaluateFeatureFlag: () => ({
        evaluateFeatureFlag: false,
    }),
    OrgFeatureFlagValue: () => ({
        organizationFeatureFlagValue: false,
    }),
    OrgFeatureFlagOverrides: () => ({
        organizationFeatureFlagOverrides: [],
    }),
    HomePanelsQuery: () => ({
        node: {
            __typename: 'User',
            recentlySearchedRepositoriesLogs: recentSearchesPayload(),
            recentSearchesLogs: recentSearchesPayload(),
            recentFilesLogs: recentFilesPayload(),
            collaborators: collaboratorsPayload(),
        },
        savedSearches: savedSearchesPayload(),
    }),
    SearchHistoryEventLogsQuery: () => ({
        currentUser: {
            __typename: 'User',
            recentSearchLogs: {
                __typename: 'EventLogsConnection',
                nodes: [],
            },
        },
    }),
    DefaultSearchContextSpec: () => ({
        defaultSearchContext: {
            __typename: 'SearchContext',
            spec: 'global',
        },
    }),
}
