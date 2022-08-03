import React, { useEffect, useState, useCallback, useMemo } from 'react'

import * as H from 'history'
import { parse as parseJSONC } from 'jsonc-parser'
import { Redirect, useHistory } from 'react-router'
import { Observable, Subject } from 'rxjs'

import { ErrorAlert } from '@sourcegraph/branded/src/components/alerts'
import { hasProperty } from '@sourcegraph/common'
import { useQuery } from '@sourcegraph/http-client'
import * as GQL from '@sourcegraph/shared/src/schema'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { LoadingSpinner, H2, H3, Badge, Container } from '@sourcegraph/wildcard'

import {
    ExternalServiceFields,
    Scalars,
    AddExternalServiceInput,
    ExternalServiceSyncJobListFields,
    ExternalServiceSyncJobConnectionFields,
    ExternalServiceResult,
    ExternalServiceVariables,
} from '../../graphql-operations'
import { FilteredConnection, FilteredConnectionQueryArguments } from '../FilteredConnection'
import { LoaderButton } from '../LoaderButton'
import { PageTitle } from '../PageTitle'
import { Duration } from '../time/Duration'
import { Timestamp } from '../time/Timestamp'

import {
    useSyncExternalService,
    queryExternalServiceSyncJobs as _queryExternalServiceSyncJobs,
    useUpdateExternalService,
    FETCH_EXTERNAL_SERVICE,
} from './backend'
import { ExternalServiceCard } from './ExternalServiceCard'
import { ExternalServiceForm } from './ExternalServiceForm'
import { defaultExternalServices, codeHostExternalServices } from './externalServices'
import { ExternalServiceWebhook } from './ExternalServiceWebhook'

interface Props extends TelemetryProps {
    externalServiceID: Scalars['ID']
    isLightTheme: boolean
    history: H.History
    afterUpdateRoute: string

    /** For testing only. */
    queryExternalServiceSyncJobs?: typeof _queryExternalServiceSyncJobs
    /** For testing only. */
    autoFocusForm?: boolean
}

function isValidURL(url: string): boolean {
    try {
        new URL(url)
        return true
    } catch {
        return false
    }
}
const getExternalService = (queryResult?: ExternalServiceResult): ExternalServiceFields | null =>
    queryResult?.node?.__typename === 'ExternalService' ? queryResult.node : null

export const ExternalServicePage: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    externalServiceID,
    history,
    isLightTheme,
    telemetryService,
    afterUpdateRoute,
    queryExternalServiceSyncJobs = _queryExternalServiceSyncJobs,
    autoFocusForm,
}) => {
    useEffect(() => {
        telemetryService.logViewEvent('SiteAdminExternalService')
    }, [telemetryService])

    const [externalService, setExternalService] = useState<ExternalServiceFields>()

    const { error: fetchError, loading: fetchLoading } = useQuery<ExternalServiceResult, ExternalServiceVariables>(
        FETCH_EXTERNAL_SERVICE,
        {
            variables: { id: externalServiceID },
            notifyOnNetworkStatusChange: false,
            fetchPolicy: 'no-cache',
            onCompleted: result => {
                const data = getExternalService(result)
                if (data) {
                    setExternalService(data)
                }
            },
        }
    )

    const [
        syncExternalService,
        { error: syncExternalServiceError, loading: syncExternalServiceLoading },
    ] = useSyncExternalService()

    const [updated, setUpdated] = useState(false)
    const [
        updateExternalService,
        { error: updateExternalServiceError, loading: updateExternalServiceLoading },
    ] = useUpdateExternalService(result => {
        setExternalService(result.updateExternalService)
        setUpdated(true)
    })

    const onSubmit = useCallback(
        async (event?: React.FormEvent<HTMLFormElement>) => {
            event?.preventDefault()

            if (externalService !== undefined) {
                await updateExternalService({
                    variables: {
                        input: {
                            id: externalService.id,
                            displayName: externalService.displayName,
                            config: externalService.config,
                        },
                    },
                })
            }
        },
        [externalService, updateExternalService]
    )

    const onChange = useCallback(
        (input: AddExternalServiceInput) => {
            if (externalService !== undefined) {
                setExternalService({
                    ...externalService,
                    ...input,
                    namespace: externalService.namespace,
                })
            }
        },
        [externalService, setExternalService]
    )

    const syncJobUpdates = useMemo(() => new Subject<void>(), [])
    const triggerSync = useCallback(
        () =>
            externalService &&
            syncExternalService({ variables: { id: externalService.id } }).then(() => {
                syncJobUpdates.next()
            }),
        [externalService, syncExternalService, syncJobUpdates]
    )

    let externalServiceCategory = externalService && defaultExternalServices[externalService.kind]
    if (
        externalService &&
        [GQL.ExternalServiceKind.GITHUB, GQL.ExternalServiceKind.GITLAB].includes(externalService.kind)
    ) {
        const parsedConfig: unknown = parseJSONC(externalService.config)
        const url =
            typeof parsedConfig === 'object' &&
            parsedConfig !== null &&
            hasProperty('url')(parsedConfig) &&
            typeof parsedConfig.url === 'string' &&
            isValidURL(parsedConfig.url)
                ? new URL(parsedConfig.url)
                : undefined
        // We have no way of finding out whether a externalservice of kind GITHUB is GitHub.com or GitHub enterprise, so we need to guess based on the URL.
        if (externalService.kind === GQL.ExternalServiceKind.GITHUB && url?.hostname !== 'github.com') {
            externalServiceCategory = codeHostExternalServices.ghe
        }
        // We have no way of finding out whether a externalservice of kind GITLAB is Gitlab.com or Gitlab self-hosted, so we need to guess based on the URL.
        if (externalService.kind === GQL.ExternalServiceKind.GITLAB && url?.hostname !== 'gitlab.com') {
            externalServiceCategory = codeHostExternalServices.gitlab
        }
    }

    const combinedError = fetchError || updateExternalServiceError
    const combinedLoading = fetchLoading || updateExternalServiceLoading

    if (updated && !combinedLoading && externalService?.warning === null) {
        return <Redirect to={afterUpdateRoute} />
    }

    return (
        <div>
            {externalService ? (
                <PageTitle title={`External service - ${externalService.displayName}`} />
            ) : (
                <PageTitle title="External service" />
            )}
            <H2>Update code host connection {combinedLoading && <LoadingSpinner inline={true} />}</H2>
            {combinedError !== undefined && !combinedLoading && <ErrorAlert className="mb-3" error={combinedError} />}

            {externalService && (
                <Container className="mb-3">
                    {externalServiceCategory && (
                        <div className="mb-3">
                            <ExternalServiceCard {...externalServiceCategory} namespace={externalService?.namespace} />
                        </div>
                    )}
                    {externalServiceCategory && (
                        <ExternalServiceForm
                            input={{ ...externalService, namespace: externalService.namespace?.id ?? null }}
                            editorActions={externalServiceCategory.editorActions}
                            jsonSchema={externalServiceCategory.jsonSchema}
                            error={updateExternalServiceError}
                            warning={externalService.warning}
                            mode="edit"
                            loading={combinedLoading}
                            onSubmit={onSubmit}
                            onChange={onChange}
                            history={history}
                            isLightTheme={isLightTheme}
                            telemetryService={telemetryService}
                            autoFocus={autoFocusForm}
                        />
                    )}
                    <LoaderButton
                        label="Trigger manual sync"
                        alwaysShowLabel={true}
                        variant="secondary"
                        onClick={triggerSync}
                        loading={syncExternalServiceLoading}
                        disabled={syncExternalServiceLoading}
                    />
                    {syncExternalServiceError && <ErrorAlert error={syncExternalServiceError} />}
                    <ExternalServiceWebhook externalService={externalService} className="mt-3" />
                    <ExternalServiceSyncJobsList
                        queryExternalServiceSyncJobs={queryExternalServiceSyncJobs}
                        externalServiceID={externalService.id}
                        updates={syncJobUpdates}
                    />
                </Container>
            )}
        </div>
    )
}

interface ExternalServiceSyncJobsListProps {
    externalServiceID: Scalars['ID']
    updates: Observable<void>

    /** For testing only. */
    queryExternalServiceSyncJobs?: typeof _queryExternalServiceSyncJobs
}

const ExternalServiceSyncJobsList: React.FunctionComponent<ExternalServiceSyncJobsListProps> = ({
    externalServiceID,
    updates,
    queryExternalServiceSyncJobs = _queryExternalServiceSyncJobs,
}) => {
    const queryConnection = useCallback(
        (args: FilteredConnectionQueryArguments) =>
            queryExternalServiceSyncJobs({
                first: args.first ?? null,
                externalService: externalServiceID,
            }),
        [externalServiceID, queryExternalServiceSyncJobs]
    )

    const history = useHistory()

    return (
        <>
            <H3 className="mt-3">Recent sync jobs</H3>
            <FilteredConnection<
                ExternalServiceSyncJobListFields,
                Omit<ExternalServiceSyncJobNodeProps, 'node'>,
                {},
                ExternalServiceSyncJobConnectionFields
            >
                className="mb-0"
                listClassName="list-group list-group-flush mb-0"
                noun="sync job"
                pluralNoun="sync jobs"
                queryConnection={queryConnection}
                nodeComponent={ExternalServiceSyncJobNode}
                nodeComponentProps={{}}
                hideSearch={true}
                noSummaryIfAllNodesVisible={true}
                history={history}
                updates={updates}
                location={history.location}
            />
        </>
    )
}

interface ExternalServiceSyncJobNodeProps {
    node: ExternalServiceSyncJobListFields
}

const ExternalServiceSyncJobNode: React.FunctionComponent<ExternalServiceSyncJobNodeProps> = ({ node }) => (
    <li className="list-group-item py-3">
        <div className="d-flex align-items-center justify-content-between">
            <div className="flex-shrink-0 mr-2">
                <Badge>{node.state}</Badge>
            </div>
            <div className="flex-shrink-0">
                {node.startedAt && (
                    <>
                        {node.finishedAt === null && <>Running since </>}
                        {node.finishedAt !== null && <>Ran for </>}
                        <Duration
                            start={node.startedAt}
                            end={node.finishedAt ?? undefined}
                            stableWidth={false}
                            className="d-inline"
                        />
                    </>
                )}
            </div>
            <div className="text-right flex-grow-1">
                <div>
                    {node.startedAt === null && 'Not started yet'}
                    {node.startedAt !== null && (
                        <>
                            Started <Timestamp date={node.startedAt} />
                        </>
                    )}
                </div>
                <div>
                    {node.finishedAt === null && 'Not finished yet'}
                    {node.finishedAt !== null && (
                        <>
                            Finished <Timestamp date={node.finishedAt} />
                        </>
                    )}
                </div>
            </div>
        </div>
        {node.failureMessage && <ErrorAlert error={node.failureMessage} className="mt-2 mb-0" />}
    </li>
)
