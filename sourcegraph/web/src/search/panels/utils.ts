import { Observable, of } from 'rxjs'
import { ISavedSearch, Namespace, IOrg, IUser } from '../../../../shared/src/graphql/schema'
import { AuthenticatedUser } from '../../auth'
import { EventLogResult } from '../backend'

export const authUser: AuthenticatedUser = {
    __typename: 'User',
    id: '0',
    email: 'alice@sourcegraph.com',
    username: 'alice',
    avatarURL: null,
    session: { canSignOut: true },
    displayName: null,
    url: '',
    settingsURL: '#',
    siteAdmin: true,
    organizations: {
        nodes: [
            { id: '0', settingsURL: '#', displayName: 'Acme Corp' },
            { id: '1', settingsURL: '#', displayName: 'Beta Inc' },
        ] as IOrg[],
    },
    tags: [],
    viewerCanAdminister: true,
    databaseID: 0,
}

export const org: IOrg = {
    __typename: 'Org',
    id: '1',
    name: 'test-org',
    displayName: 'test org',
    createdAt: '2020-01-01',
    members: {
        __typename: 'UserConnection',
        nodes: [authUser] as IUser[],
        totalCount: 1,
        pageInfo: { __typename: 'PageInfo', endCursor: null, hasNextPage: false },
    },
    latestSettings: null,
    settingsCascade: {
        __typename: 'SettingsCascade',
        subjects: [],
        final: '',
        merged: { __typename: 'Configuration', contents: '', messages: [] },
    },
    configurationCascade: {
        __typename: 'ConfigurationCascade',
        subjects: [],
        merged: { __typename: 'Configuration', contents: '', messages: [] },
    },
    viewerPendingInvitation: null,
    viewerCanAdminister: true,
    viewerIsMember: true,
    url: '/organizations/test-org',
    settingsURL: '/organizations/test-org/settings',
    namespaceName: 'test-org',
    campaigns: {
        __typename: 'CampaignConnection',
        nodes: [],
        totalCount: 0,
        pageInfo: { __typename: 'PageInfo', endCursor: null, hasNextPage: false },
    },
}

export const _fetchSavedSearches = (): Observable<ISavedSearch[]> =>
    of([
        {
            __typename: 'SavedSearch',
            id: 'test',
            description: 'test',
            query: 'test',
            notify: false,
            notifySlack: false,
            namespace: authUser as Namespace,
            slackWebhookURL: null,
        },
        {
            __typename: 'SavedSearch',
            id: 'test-org',
            description: 'org test',
            query: 'org test',
            notify: false,
            notifySlack: false,
            namespace: org,
            slackWebhookURL: null,
        },
    ])

export const _fetchRecentSearches = (): Observable<EventLogResult | null> =>
    of({
        nodes: [
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 4, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 0, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "test"}}}',
                timestamp: '2020-09-08T17:36:52Z',
                url: 'https://sourcegraph.test:3443/search?q=test&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 5, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 1, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "^test"}}}',
                timestamp: '2020-09-08T17:26:05Z',
                url: 'https://sourcegraph.test:3443/search?q=%5Etest&patternType=regexp',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 5, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 1, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "^test"}}}',
                timestamp: '2020-09-08T17:20:11Z',
                url: 'https://sourcegraph.test:3443/search?q=%5Etest&patternType=regexp',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 5, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 1, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "^test"}}}',
                timestamp: '2020-09-08T17:20:05Z',
                url: 'https://sourcegraph.test:3443/search?q=%5Etest&patternType=regexp',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 26, "space": 2, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 3, "count_non_default": 1}, "field_lang": {"count": 1, "count_alias": 0, "count_negated": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 2, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "lang:cpp try {:[my_match]}"}}}',
                timestamp: '2020-09-08T17:12:53Z',
                url:
                    'https://sourcegraph.test:3443/search?q=lang:cpp+try+%7B:%5Bmy_match%5D%7D&patternType=structural&onboardingTour=true',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 26, "space": 2, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 3, "count_non_default": 1}, "field_lang": {"count": 1, "count_alias": 0, "count_negated": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 2, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "lang:cpp try {:[my_match]}"}}}',
                timestamp: '2020-09-08T17:11:46Z',
                url:
                    'https://sourcegraph.test:3443/search?q=lang:cpp+try+%7B:%5Bmy_match%5D%7D&patternType=structural&onboardingTour=true',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 86, "space": 4, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 4, "count_non_default": 3}, "field_lang": {"count": 1, "count_alias": 0, "count_negated": 0}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 1, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_type": {"count": 1, "value_diff": 0, "value_file": 0, "value_commit": 1, "value_symbol": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 1, "count_pattern": 1, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:^github\\\\.com/sourcegraph/sourcegraph$ PanelContainer lang:typescript  type:commit"}}}',
                timestamp: '2020-09-04T20:31:57Z',
                url:
                    'https://sourcegraph.test:3443/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+PanelContainer+lang:typescript++type:commit&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 86, "space": 4, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 4, "count_non_default": 3}, "field_lang": {"count": 1, "count_alias": 0, "count_negated": 0}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 1, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_type": {"count": 1, "value_diff": 0, "value_file": 0, "value_commit": 1, "value_symbol": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 1, "count_pattern": 1, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:^github\\\\.com/sourcegraph/sourcegraph$ PanelContainer lang:typescript  type:commit"}}}',
                timestamp: '2020-09-04T20:27:02Z',
                url:
                    'https://sourcegraph.test:3443/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+PanelContainer+lang:typescript++type:commit&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 86, "space": 4, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 4, "count_non_default": 3}, "field_lang": {"count": 1, "count_alias": 0, "count_negated": 0}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 1, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_type": {"count": 1, "value_diff": 0, "value_file": 0, "value_commit": 1, "value_symbol": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 1, "count_pattern": 1, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:^github\\\\.com/sourcegraph/sourcegraph$ PanelContainer lang:typescript  type:commit"}}}',
                timestamp: '2020-09-04T20:24:56Z',
                url:
                    'https://sourcegraph.test:3443/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+PanelContainer+lang:typescript++type:commit&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 74, "space": 3, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 3, "count_non_default": 2}, "field_lang": {"count": 1, "count_alias": 0, "count_negated": 0}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 1, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 1, "count_pattern": 1, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:^github\\\\.com/sourcegraph/sourcegraph$ PanelContainer lang:typescript "}}}',
                timestamp: '2020-09-04T20:23:44Z',
                url:
                    'https://sourcegraph.test:3443/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+PanelContainer+lang:typescript+&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 57, "space": 1, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 2, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 1, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 2, "count_regexp": 0, "count_literal": 1, "count_pattern": 1, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:^github\\\\.com/sourcegraph/sourcegraph$ PanelContainer"}}}',
                timestamp: '2020-09-04T20:23:38Z',
                url:
                    'https://sourcegraph.test:3443/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+PanelContainer&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 43, "space": 1, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 1, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 1, "count_regexp": 0, "count_literal": 0, "count_pattern": 1, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:^github\\\\.com/sourcegraph/sourcegraph$ "}}}',
                timestamp: '2020-09-04T20:23:30Z',
                url:
                    'https://sourcegraph.test:3443/search?q=repo:%5Egithub%5C.com/sourcegraph/sourcegraph%24+&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 28, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 0, "value_regexp": 0, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 0, "count_regexp": 0, "count_literal": 0, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "repo:sourcegraph/sourcegraph"}}}',
                timestamp: '2020-09-04T20:23:23Z',
                url: 'https://sourcegraph.test:3443/search?q=repo:sourcegraph/sourcegraph&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 4, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 0, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "test"}}}',
                timestamp: '2020-09-04T20:23:09Z',
                url: 'https://sourcegraph.test:3443/search?q=test&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 13, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 1, "value_regexp": 0, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 0, "count_regexp": 0, "count_literal": 0, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "r:sourcegraph"}}}',
                timestamp: '2020-09-04T20:23:08Z',
                url: 'https://sourcegraph.test:3443/search?q=r:sourcegraph&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 4, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 0, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "test"}}}',
                timestamp: '2020-09-04T20:23:07Z',
                url: 'https://sourcegraph.test:3443/search?q=test&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 13, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 1, "value_regexp": 0, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 0, "count_regexp": 0, "count_literal": 0, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "r:sourcegraph"}}}',
                timestamp: '2020-09-04T20:23:06Z',
                url: 'https://sourcegraph.test:3443/search?q=r:sourcegraph&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 4, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 0}, "field_default": {"count": 1, "count_regexp": 0, "count_literal": 1, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "test"}}}',
                timestamp: '2020-09-04T20:23:06Z',
                url: 'https://sourcegraph.test:3443/search?q=test&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 13, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 1, "value_regexp": 0, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 0, "count_regexp": 0, "count_literal": 0, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "r:sourcegraph"}}}',
                timestamp: '2020-09-04T18:44:39Z',
                url: 'https://sourcegraph.test:3443/search?q=r:sourcegraph&patternType=literal',
            },
            {
                argument:
                    '{"mode": "plain", "code_search": {"query_data": {"empty": false, "query": {"chars": {"count": 13, "space": 0, "non_ascii": 0, "double_quote": 0, "single_quote": 0}, "fields": {"count": 1, "count_non_default": 1}, "field_repo": {"count": 1, "value_glob": 0, "value_pipe": 0, "count_alias": 1, "value_regexp": 0, "count_negated": 0, "value_at_sign": 0, "value_rev_star": 0, "value_rev_caret": 0, "value_rev_colon": 0}, "field_default": {"count": 0, "count_regexp": 0, "count_literal": 0, "count_pattern": 0, "count_double_quote": 0, "count_single_quote": 0}}, "combined": "r:sourcegraph"}}}',
                timestamp: '2020-09-04T18:44:30Z',
                url: 'https://sourcegraph.test:3443/search?q=r:sourcegraph&patternType=literal',
            },
        ],
        pageInfo: {
            endCursor: null,
            hasNextPage: true,
        },
        totalCount: 436,
    })
