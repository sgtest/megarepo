import React, { FunctionComponent, useCallback, useEffect } from 'react'
import { RouteComponentProps } from 'react-router'
import * as GQL from '../../../../../shared/src/graphql/schema'
import { TelemetryProps } from '../../../../../shared/src/telemetry/telemetryService'
import {
    FilteredConnection,
    FilteredConnectionFilter,
    FilteredConnectionQueryArguments,
} from '../../../components/FilteredConnection'
import { LsifUploadFields, LSIFUploadState } from '../../../graphql-operations'
import { fetchLsifUploads as defaultFetchLsifUploads } from './backend'
import { CodeIntelUploadNode, CodeIntelUploadNodeProps } from './CodeIntelUploadNode'
import { CodeIntelUploadsPageTitle } from './CodeIntelUploadsPageTitle'

export interface CodeIntelUploadsPageProps extends RouteComponentProps<{}>, TelemetryProps {
    repo?: GQL.IRepository
    fetchLsifUploads?: typeof defaultFetchLsifUploads
    now?: () => Date
}

const filters: FilteredConnectionFilter[] = [
    {
        label: 'All',
        id: 'all',
        tooltip: 'Show all uploads',
        args: {},
    },
    {
        label: 'Current',
        id: 'current',
        tooltip: 'Show current uploads only',
        args: { isLatestForRepo: true },
    },
    {
        label: 'Completed',
        id: 'completed',
        tooltip: 'Show completed uploads only',
        args: { state: LSIFUploadState.COMPLETED },
    },
    {
        label: 'Errored',
        id: 'errored',
        tooltip: 'Show errored uploads only',
        args: { state: LSIFUploadState.ERRORED },
    },
    {
        label: 'Queued',
        id: 'queued',
        tooltip: 'Show queued uploads only',
        args: { state: LSIFUploadState.QUEUED },
    },
]

export const CodeIntelUploadsPage: FunctionComponent<CodeIntelUploadsPageProps> = ({
    repo,
    fetchLsifUploads = defaultFetchLsifUploads,
    now,
    telemetryService,
    ...props
}) => {
    useEffect(() => telemetryService.logViewEvent('CodeIntelUploads'), [telemetryService])

    const queryUploads = useCallback(
        (args: FilteredConnectionQueryArguments) => fetchLsifUploads({ repository: repo?.id, ...args }),
        [repo?.id, fetchLsifUploads]
    )

    return (
        <div className="code-intel-uploads">
            <CodeIntelUploadsPageTitle />

            <div className="list-group position-relative">
                <FilteredConnection<LsifUploadFields, Omit<CodeIntelUploadNodeProps, 'node'>>
                    className="mt-2"
                    listComponent="div"
                    listClassName="codeintel-uploads__grid mb-3"
                    noun="upload"
                    pluralNoun="uploads"
                    nodeComponent={CodeIntelUploadNode}
                    nodeComponentProps={{ now }}
                    queryConnection={queryUploads}
                    history={props.history}
                    location={props.location}
                    cursorPaging={true}
                    filters={filters}
                    defaultFilter="current"
                />
            </div>
        </div>
    )
}
