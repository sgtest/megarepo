import React, { useEffect, useMemo, useCallback, useState } from 'react'

import { mdiPlus } from '@mdi/js'
import * as H from 'history'
import { Redirect } from 'react-router'
import { Subject } from 'rxjs'

import { isErrorLike, ErrorLike } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Link, ButtonLink, Icon, PageHeader, Container } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../auth'
import { ListExternalServiceFields, Scalars, ExternalServicesResult } from '../../graphql-operations'
import { FilteredConnection, FilteredConnectionQueryArguments } from '../FilteredConnection'
import { PageTitle } from '../PageTitle'

import { queryExternalServices as _queryExternalServices } from './backend'
import { ExternalServiceEditingDisabledAlert } from './ExternalServiceEditingDisabledAlert'
import { ExternalServiceEditingTemporaryAlert } from './ExternalServiceEditingTemporaryAlert'
import { ExternalServiceNodeProps, ExternalServiceNode } from './ExternalServiceNode'

interface Props extends TelemetryProps {
    history: H.History
    location: H.Location
    routingPrefix: string
    afterDeleteRoute: string
    userID?: Scalars['ID']
    authenticatedUser: Pick<AuthenticatedUser, 'id'>

    externalServicesFromFile: boolean
    allowEditExternalServicesWithFile: boolean

    /** For testing only. */
    queryExternalServices?: typeof _queryExternalServices
}

/**
 * A page displaying the external services on this site.
 */
export const ExternalServicesPage: React.FunctionComponent<React.PropsWithChildren<Props>> = ({
    afterDeleteRoute,
    history,
    location,
    routingPrefix,
    userID,
    telemetryService,
    authenticatedUser,
    externalServicesFromFile,
    allowEditExternalServicesWithFile,
    queryExternalServices = _queryExternalServices,
}) => {
    const POLLING_INTERVAL = 15000
    const updates = useMemo(() => new Subject<void>(), [])

    useEffect(() => {
        telemetryService.logViewEvent('SiteAdminExternalServices')
        const interval = setInterval(() => {
            updates.next()
        }, POLLING_INTERVAL)
        return () => clearInterval(interval)
    }, [updates, telemetryService])

    const onDidUpdateExternalServices = useCallback(() => updates.next(), [updates])

    const queryConnection = useCallback(
        (args: FilteredConnectionQueryArguments) =>
            queryExternalServices({
                first: args.first ?? null,
                after: args.after ?? null,
            }),
        [queryExternalServices]
    )

    const [noExternalServices, setNoExternalServices] = useState<boolean>(false)
    const onUpdate = useCallback<
        (connection: ExternalServicesResult['externalServices'] | ErrorLike | undefined) => void
    >(connection => {
        if (connection && !isErrorLike(connection)) {
            setNoExternalServices(connection.totalCount === 0)
        }
    }, [])

    const editingDisabled = !!externalServicesFromFile && !allowEditExternalServicesWithFile

    const isManagingOtherUser = !!userID && userID !== authenticatedUser.id

    if (!isManagingOtherUser && noExternalServices) {
        return <Redirect to={`${routingPrefix}/external-services/new`} />
    }
    return (
        <div className="site-admin-external-services-page">
            <PageTitle title="Manage code hosts" />
            <PageHeader
                path={[{ text: 'Manage code hosts' }]}
                description="Manage code host connections to sync repositories."
                headingElement="h2"
                actions={
                    <>
                        {!isManagingOtherUser && (
                            <ButtonLink
                                className="test-goto-add-external-service-page"
                                to={`${routingPrefix}/external-services/new`}
                                variant="primary"
                                as={Link}
                                disabled={editingDisabled}
                            >
                                <Icon aria-hidden={true} svgPath={mdiPlus} /> Add code host
                            </ButtonLink>
                        )}
                    </>
                }
                className="mb-3"
            />

            {editingDisabled && <ExternalServiceEditingDisabledAlert />}
            {externalServicesFromFile && allowEditExternalServicesWithFile && <ExternalServiceEditingTemporaryAlert />}

            <Container className="mb-3">
                <FilteredConnection<
                    ListExternalServiceFields,
                    Omit<ExternalServiceNodeProps, 'node'>,
                    {},
                    ExternalServicesResult['externalServices']
                >
                    className="mb-0"
                    listClassName="list-group list-group-flush mb-0"
                    noun="code host"
                    pluralNoun="code hosts"
                    withCenteredSummary={true}
                    queryConnection={queryConnection}
                    nodeComponent={ExternalServiceNode}
                    nodeComponentProps={{
                        onDidUpdate: onDidUpdateExternalServices,
                        history,
                        routingPrefix,
                        afterDeleteRoute,
                        editingDisabled,
                    }}
                    hideSearch={true}
                    cursorPaging={true}
                    updates={updates}
                    history={history}
                    location={location}
                    onUpdate={onUpdate}
                />
            </Container>
        </div>
    )
}
