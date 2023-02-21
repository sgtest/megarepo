import { FC, useEffect, useState } from 'react'

import { mdiCog } from '@mdi/js'
import { noop } from 'lodash'
import { useNavigate } from 'react-router-dom'

import { useMutation } from '@sourcegraph/http-client'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Container, ErrorAlert, Form, Input, PageHeader } from '@sourcegraph/wildcard'

import { PageTitle } from '../../components/PageTitle'
import { CreateOutboundWebhookResult, CreateOutboundWebhookVariables } from '../../graphql-operations'
import { generateSecret } from '../../util/security'

import { CREATE_OUTBOUND_WEBHOOK } from './backend'
import { EventTypes } from './create-edit/EventTypes'
import { SubmitButton } from './create-edit/SubmitButton'

export interface CreatePageProps extends TelemetryProps {}

export const CreatePage: FC<CreatePageProps> = ({ telemetryService }) => {
    const navigate = useNavigate()
    useEffect(() => {
        telemetryService.logPageView('OutboundWebhooksCreatePage')
    }, [telemetryService])

    const [url, setURL] = useState('')
    const [secret, setSecret] = useState(generateSecret())
    const [eventTypes, setEventTypes] = useState<Set<string>>(new Set())

    const [createWebhook, { error: createError, loading }] = useMutation<
        CreateOutboundWebhookResult,
        CreateOutboundWebhookVariables
    >(CREATE_OUTBOUND_WEBHOOK, {
        variables: {
            input: {
                eventTypes: [...eventTypes].map(eventType => ({
                    eventType,
                })),
                secret,
                url,
            },
        },
        onCompleted: () => navigate('/site-admin/outbound-webhooks'),
    })

    return (
        <div>
            <PageTitle title="Add outgoing webhook" />
            <PageHeader
                path={[
                    { icon: mdiCog },
                    { to: '/site-admin/outbound-webhooks', text: 'Outgoing webhooks' },
                    { to: '/site-admin/outbound-webhooks/create', text: 'Add' },
                ]}
                headingElement="h2"
                description="Add a new outgoing webhook"
                className="mb-3"
            />

            <Container>
                {createError && <ErrorAlert error={createError} />}
                <Form>
                    <Input label="URL" required={true} value={url} onChange={event => setURL(event.target.value)} />
                    <Input
                        label="Secret"
                        message={<small>Randomly generated. Alter as required.</small>}
                        required={true}
                        value={secret}
                        onChange={event => setSecret(event.target.value)}
                    />
                    <EventTypes className="border-top pt-2" values={eventTypes} onChange={setEventTypes} />
                    <SubmitButton
                        onClick={() => {
                            createWebhook().catch(noop)
                        }}
                        state={loading ? 'loading' : eventTypes.size === 0 ? 'disabled' : undefined}
                    >
                        Create
                    </SubmitButton>
                </Form>
            </Container>
        </div>
    )
}
