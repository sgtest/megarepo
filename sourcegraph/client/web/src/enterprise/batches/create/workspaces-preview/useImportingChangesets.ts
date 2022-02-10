import { dataOrThrowErrors } from '@sourcegraph/http-client'
import {
    useConnection,
    UseConnectionResult,
} from '@sourcegraph/web/src/components/FilteredConnection/hooks/useConnection'

import {
    Scalars,
    PreviewBatchSpecImportingChangesetFields,
    BatchSpecImportingChangesetsResult,
    BatchSpecImportingChangesetsVariables,
} from '../../../../graphql-operations'
import { IMPORTING_CHANGESETS } from '../backend'

export const CHANGESETS_PER_PAGE_COUNT = 100

export type ImportingChangesetFields =
    | PreviewBatchSpecImportingChangesetFields
    | { __typename: 'HiddenChangesetSpec'; id: Scalars['ID'] }

/**
 * Custom hook to query the connection of changesets a batch spec will import when run.
 *
 * @param batchSpecID The id of the batch spec to query
 */
export const useImportingChangesets = (batchSpecID: Scalars['ID']): UseConnectionResult<ImportingChangesetFields> =>
    useConnection<
        BatchSpecImportingChangesetsResult,
        BatchSpecImportingChangesetsVariables,
        PreviewBatchSpecImportingChangesetFields | { __typename: 'HiddenChangesetSpec'; id: Scalars['ID'] }
    >({
        query: IMPORTING_CHANGESETS,
        variables: {
            batchSpec: batchSpecID,
            after: null,
            first: CHANGESETS_PER_PAGE_COUNT,
        },
        options: {
            useURL: false,
            fetchPolicy: 'cache-and-network',
        },
        getConnection: result => {
            const data = dataOrThrowErrors(result)

            if (!data.node) {
                throw new Error(`Batch spec with ID ${batchSpecID} does not exist`)
            }
            if (data.node.__typename !== 'BatchSpec') {
                throw new Error(`The given ID is a ${data.node.__typename as string}, not a BatchSpec`)
            }
            if (!data.node.importingChangesets) {
                return { nodes: [] }
            }
            return data.node.importingChangesets
        },
    })
