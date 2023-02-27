import { gql } from '@sourcegraph/http-client'

import { gitCommitFragment } from '../../commits/RepositoryCommitsPage'

const OWNER_FIELDS = gql`
    fragment OwnerFields on Person {
        email
        avatarURL
        displayName
        user {
            username
            displayName
            url
        }
    }
`

export const FETCH_OWNERS = gql`
    ${OWNER_FIELDS}

    fragment CodeownersFileEntryFields on CodeownersFileEntry {
        title
        description
    }

    query FetchOwnership($repo: ID!, $revision: String!, $currentPath: String!) {
        node(id: $repo) {
            ... on Repository {
                commit(rev: $revision) {
                    blob(path: $currentPath) {
                        ownership {
                            nodes {
                                owner {
                                    ...OwnerFields
                                }
                                reasons {
                                    ...CodeownersFileEntryFields
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
