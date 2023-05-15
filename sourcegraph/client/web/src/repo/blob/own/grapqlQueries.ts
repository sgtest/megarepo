import { gql } from '@sourcegraph/http-client'

import { gitCommitFragment } from '../../commits/RepositoryCommitsPage'

export const OWNER_FIELDS = gql`
    fragment OwnerFields on Owner {
        __typename
        ... on Person {
            displayName
            email
            avatarURL
            user {
                username
                displayName
                url
                primaryEmail {
                    email
                }
            }
        }
        ... on Team {
            name
            teamDisplayName: displayName
            avatarURL
            url
        }
    }
`

export const RECENT_CONTRIBUTOR_FIELDS = gql`
    fragment RecentContributorOwnershipSignalFields on RecentContributorOwnershipSignal {
        title
        description
    }
`

export const RECENT_VIEW_FIELDS = gql`
    fragment RecentViewOwnershipSignalFields on RecentViewOwnershipSignal {
        title
        description
    }
`

export const FETCH_OWNERS = gql`
    ${OWNER_FIELDS}
    ${RECENT_CONTRIBUTOR_FIELDS}
    ${RECENT_VIEW_FIELDS}

    fragment CodeownersFileEntryFields on CodeownersFileEntry {
        title
        description
        codeownersFile {
            url
        }
        ruleLineMatch
    }

    query FetchOwnership($repo: ID!, $revision: String!, $currentPath: String!) {
        node(id: $repo) {
            ... on Repository {
                commit(rev: $revision) {
                    blob(path: $currentPath) {
                        ownership {
                            totalOwners
                            nodes {
                                owner {
                                    ...OwnerFields
                                }
                                reasons {
                                    ...CodeownersFileEntryFields
                                    ...RecentContributorOwnershipSignalFields
                                    ...RecentViewOwnershipSignalFields
                                }
                            }
                        }
                    }
                }
            }
        }
    }
`

export const FETCH_OWNERS_AND_HISTORY = gql`
    ${OWNER_FIELDS}
    ${gitCommitFragment}

    query FetchOwnersAndHistory($repo: ID!, $revision: String!, $currentPath: String!) {
        node(id: $repo) {
            ... on Repository {
                sourceType
                commit(rev: $revision) {
                    blob(path: $currentPath) {
                        ownership(first: 2) {
                            nodes {
                                owner {
                                    ...OwnerFields
                                }
                            }
                            totalCount
                        }
                    }
                    ancestors(first: 1, path: $currentPath) {
                        nodes {
                            ...GitCommitFields
                        }
                    }
                }
            }
        }
    }
`
