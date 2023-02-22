import { FC, useEffect, useCallback, useState } from 'react'

import { useNavigate } from 'react-router-dom'

import { asError, isErrorLike, logger, renderMarkdown } from '@sourcegraph/common'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Alert, Container, H2, H3, H4, Markdown } from '@sourcegraph/wildcard'

import { ExternalServiceFields, AddExternalServiceInput } from '../../graphql-operations'
import { refreshSiteFlags } from '../../site/backend'
import { PageTitle } from '../PageTitle'

import { addExternalService } from './backend'
import { ExternalServiceCard } from './ExternalServiceCard'
import { ExternalServiceForm } from './ExternalServiceForm'
import { AddExternalServiceOptions } from './externalServices'

interface Props extends TelemetryProps {
    externalService: AddExternalServiceOptions
    externalServicesFromFile: boolean
    allowEditExternalServicesWithFile: boolean

    /** For testing only. */
    autoFocusForm?: boolean
}

/**
 * Page for adding a single external service.
 */
export const AddExternalServicePage: FC<Props> = ({
    externalService,
    telemetryService,
    autoFocusForm,
    externalServicesFromFile,
    allowEditExternalServicesWithFile,
}) => {
    const [config, setConfig] = useState(externalService.defaultConfig)
    const [displayName, setDisplayName] = useState(externalService.defaultDisplayName)
    const navigate = useNavigate()

    useEffect(() => {
        telemetryService.logPageView('AddExternalService')
    }, [telemetryService])

    const getExternalServiceInput = useCallback(
        (): AddExternalServiceInput => ({
            displayName,
            config,
            kind: externalService.kind,
        }),
        [displayName, config, externalService.kind]
    )

    const onChange = useCallback(
        (input: AddExternalServiceInput): void => {
            setDisplayName(input.displayName)
            setConfig(input.config)
        },
        [setDisplayName, setConfig]
    )

    const [isCreating, setIsCreating] = useState<boolean | Error>(false)
    const [createdExternalService, setCreatedExternalService] = useState<ExternalServiceFields>()
    const onSubmit = useCallback(
        async (event?: React.FormEvent<HTMLFormElement>): Promise<void> => {
            if (event) {
                event.preventDefault()
            }
            setIsCreating(true)
            try {
                const service = await addExternalService({ input: { ...getExternalServiceInput() } }, telemetryService)
                setIsCreating(false)
                setCreatedExternalService(service)
            } catch (error) {
                setIsCreating(asError(error))
            }
        },
        [getExternalServiceInput, telemetryService]
    )

    useEffect(() => {
        if (createdExternalService && !isErrorLike(createdExternalService)) {
            // Refresh site flags so that global site alerts
            // reflect the latest configuration.
            // eslint-disable-next-line rxjs/no-ignored-subscription
            refreshSiteFlags().subscribe({ error: error => logger.error(error) })
            navigate(`/site-admin/external-services/${createdExternalService.id}`)
        }
    }, [createdExternalService, navigate])

    return (
        <>
            <PageTitle title="Add repositories" />
            <H2>Add repositories</H2>
            <Container>
                {createdExternalService?.warning ? (
                    <div>
                        <div className="mb-3">
                            <ExternalServiceCard
                                {...externalService}
                                title={createdExternalService.displayName}
                                shortDescription="Update this external service configuration to manage repository mirroring."
                                to={`/site-admin/external-services/${createdExternalService.id}/edit`}
                            />
                        </div>
                        <Alert variant="warning">
                            <H4>Warning</H4>
                            <Markdown dangerousInnerHTML={renderMarkdown(createdExternalService.warning)} />
                        </Alert>
                    </div>
                ) : (
                    <>
                        <div className="mb-3">
                            <ExternalServiceCard {...externalService} />
                        </div>
                        <H3>Instructions:</H3>
                        <div className="mb-4">{externalService.instructions}</div>
                        <ExternalServiceForm
                            telemetryService={telemetryService}
                            error={isErrorLike(isCreating) ? isCreating : undefined}
                            input={getExternalServiceInput()}
                            editorActions={externalService.editorActions}
                            jsonSchema={externalService.jsonSchema}
                            mode="create"
                            onSubmit={onSubmit}
                            onChange={onChange}
                            loading={isCreating === true}
                            autoFocus={autoFocusForm}
                            externalServicesFromFile={externalServicesFromFile}
                            allowEditExternalServicesWithFile={allowEditExternalServicesWithFile}
                        />
                    </>
                )}
            </Container>
        </>
    )
}
