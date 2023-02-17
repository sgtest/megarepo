import { gql } from '@sourcegraph/http-client'

export const lsifIndexFieldsFragment = gql`
    fragment LsifIndexFields on LSIFIndex {
        __typename
        id
        inputCommit
        tags
        inputRoot
        inputIndexer
        indexer {
            name
            url
        }
        projectRoot {
            url
            path
            repository {
                url
                name
            }
            commit {
                url
                oid
                abbreviatedOID
            }
        }
        steps {
            ...LsifIndexStepsFields
        }
        state
        failure
        queuedAt
        startedAt
        finishedAt
        placeInQueue
        associatedUpload {
            id
            state
            uploadedAt
            startedAt
            finishedAt
            placeInQueue
        }
        shouldReindex
    }

    fragment LsifIndexStepsFields on IndexSteps {
        setup {
            ...ExecutionLogEntryFields
        }
        preIndex {
            root
            image
            commands
            logEntry {
                ...ExecutionLogEntryFields
            }
        }
        index {
            indexerArgs
            outfile
            logEntry {
                ...ExecutionLogEntryFields
            }
        }
        upload {
            ...ExecutionLogEntryFields
        }
        teardown {
            ...ExecutionLogEntryFields
        }
    }

    fragment ExecutionLogEntryFields on ExecutionLogEntry {
        key
        command
        startTime
        exitCode
        out
        durationMilliseconds
    }
`

export const lsifUploadAuditLogsFieldsFragment = gql`
    fragment LsifUploadsAuditLogsFields on LSIFUploadAuditLog {
        logTimestamp
        reason
        changedColumns {
            column
            old
            new
        }
        operation
    }
`

export const lsifUploadFieldsFragment = gql`
    fragment LsifUploadFields on LSIFUpload {
        __typename
        id
        inputCommit
        tags
        inputRoot
        inputIndexer
        indexer {
            name
            url
        }
        projectRoot {
            url
            path
            repository {
                url
                name
            }
            commit {
                url
                oid
                abbreviatedOID
            }
        }
        state
        failure
        isLatestForRepo
        uploadedAt
        startedAt
        finishedAt
        placeInQueue
        associatedIndex {
            id
            state
            queuedAt
            startedAt
            finishedAt
            placeInQueue
        }
        auditLogs {
            ...LsifUploadsAuditLogsFields
        }
    }

    ${lsifUploadAuditLogsFieldsFragment}
`

export const lsifUploadConnectionFieldsFragment = gql`
    fragment LsifUploadConnectionFields on LSIFUploadConnection {
        nodes {
            ...LsifUploadFields
        }
        totalCount
        pageInfo {
            endCursor
            hasNextPage
        }
    }

    ${lsifUploadFieldsFragment}
`

export const codeIntelStatusQuery = gql`
    query CodeIntelStatus($repository: String!, $commit: String!, $path: String!) {
        repository(name: $repository) {
            codeIntelSummary {
                lastIndexScan
                lastUploadRetentionScan
                recentUploads {
                    ...LSIFUploadsWithRepositoryNamespaceFields
                }
                recentIndexes {
                    ...LSIFIndexesWithRepositoryNamespaceFields
                }
                availableIndexers {
                    ...InferredAvailableIndexersFields
                }
            }
            commit(rev: $commit) {
                path(path: $path) {
                    ... on GitBlob {
                        codeIntelSupport {
                            ...CodeIntelSupportFields
                        }
                        lsif {
                            lsifUploads {
                                ...LsifUploadFields
                            }
                        }
                    }
                    ... on GitTree {
                        codeIntelInfo {
                            ...GitTreeCodeIntelInfoFields
                        }
                        lsif {
                            lsifUploads {
                                ...LsifUploadFields
                            }
                        }
                    }
                }
            }
        }
    }

    fragment CodeIntelSupportFields on CodeIntelSupport {
        preciseSupport {
            ...PreciseSupportFields
        }
        searchBasedSupport {
            ...SearchBasedCodeIntelSupportFields
        }
    }

    fragment GitTreeCodeIntelInfoFields on GitTreeCodeIntelInfo {
        preciseSupport {
            coverage {
                support {
                    ...PreciseSupportFields
                }
                confidence
            }
        }
        searchBasedSupport {
            support {
                ...SearchBasedCodeIntelSupportFields
            }
        }
    }

    fragment PreciseSupportFields on PreciseCodeIntelSupport {
        supportLevel
        indexers {
            ...CodeIntelIndexerFields
        }
    }

    fragment SearchBasedCodeIntelSupportFields on SearchBasedCodeIntelSupport {
        language
        supportLevel
    }

    fragment CodeIntelIndexerFields on CodeIntelIndexer {
        name
        url
    }

    fragment LSIFUploadsWithRepositoryNamespaceFields on LSIFUploadsWithRepositoryNamespace {
        root
        indexer {
            ...CodeIntelIndexerFields
        }
        uploads {
            ...LsifUploadFields
        }
    }

    fragment LSIFIndexesWithRepositoryNamespaceFields on LSIFIndexesWithRepositoryNamespace {
        root
        indexer {
            ...CodeIntelIndexerFields
        }
        indexes {
            ...LsifIndexFields
        }
    }

    fragment InferredAvailableIndexersFields on InferredAvailableIndexers {
        roots
        indexer {
            name
            url
        }
    }

    ${lsifUploadFieldsFragment}
    ${lsifIndexFieldsFragment}
`

export const requestedLanguageSupportQuery = gql`
    query RequestedLanguageSupport {
        requestedLanguageSupport
    }
`

export const requestLanguageSupportQuery = gql`
    mutation RequestLanguageSupport($language: String!) {
        requestLanguageSupport(language: $language) {
            alwaysNil
        }
    }
`
