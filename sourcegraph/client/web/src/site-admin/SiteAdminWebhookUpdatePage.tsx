import { FC, useEffect } from 'react'

import { mdiCog } from '@mdi/js'
import { useParams } from 'react-router-dom'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Container, PageHeader } from '@sourcegraph/wildcard'

import { CreatedByAndUpdatedByInfoByline } from '../components/Byline/CreatedByAndUpdatedByInfoByline'
import { ConnectionLoading } from '../components/FilteredConnection/ui'
import { PageTitle } from '../components/PageTitle'

import { useWebhookQuery } from './backend'
import { WebhookCreateUpdatePage } from './WebhookCreateUpdatePage'

export interface SiteAdminWebhookUpdatePageProps extends TelemetryProps {}

export const SiteAdminWebhookUpdatePage: FC<SiteAdminWebhookUpdatePageProps> = ({ telemetryService }) => {
    useEffect(() => {
        telemetryService.logPageView('SiteAdminWebhookUpdatePage')
    }, [telemetryService])

    const { id = '' } = useParams<{ id: string }>()

    const { loading, data } = useWebhookQuery(id)

    const webhook = data?.node && data.node.__typename === 'Webhook' ? data.node : undefined
    return (
        <Container>
            <PageTitle title="Incoming webhook" />
            {loading && !data && <ConnectionLoading />}
            {webhook && (
                <>
                    <PageHeader
                        path={[
                            { icon: mdiCog },
                            { to: '/site-admin/webhooks', text: 'Incoming webhooks' },
                            { text: webhook.name },
                        ]}
                        byline={
                            <CreatedByAndUpdatedByInfoByline
                                createdAt={webhook.createdAt}
                                createdBy={webhook.createdBy}
                                updatedAt={webhook.updatedAt}
                                updatedBy={webhook.updatedBy}
                            />
                        }
                        className="mb-3"
                        headingElement="h2"
                    />
                    <WebhookCreateUpdatePage existingWebhook={webhook} />
                </>
            )}
        </Container>
    )
}
