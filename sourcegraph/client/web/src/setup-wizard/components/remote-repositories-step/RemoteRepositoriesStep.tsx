import { FC, HTMLAttributes, useState, useEffect } from 'react'

import { useQuery } from '@apollo/client'
import classNames from 'classnames'
import { Routes, Route, matchPath, useLocation } from 'react-router-dom'

import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { Container, Text } from '@sourcegraph/wildcard'

import { GetCodeHostsResult } from '../../../graphql-operations'
import { CodeHostExternalServiceAlert } from '../CodeHostExternalServiceAlert'
import { ProgressBar } from '../ProgressBar'
import { FooterWidget, CustomNextButton } from '../setup-steps'

import { CodeHostDeleteModal, CodeHostToDelete } from './components/code-host-delete-modal'
import { CodeHostsPicker } from './components/code-host-picker'
import { CodeHostCreation, CodeHostEdit } from './components/code-hosts'
import { CodeHostsNavigation } from './components/navigation'
import { getNextButtonLabel, getNextButtonLogEvent, isAnyConnectedCodeHosts } from './helpers'
import { GET_CODE_HOSTS } from './queries'

import styles from './RemoteRepositoriesStep.module.scss'

interface RemoteRepositoriesStepProps extends TelemetryProps, HTMLAttributes<HTMLDivElement> {
    baseURL: string
    description?: boolean
    progressBar?: boolean
}

export const RemoteRepositoriesStep: FC<RemoteRepositoriesStepProps> = ({
    className,
    telemetryService,
    baseURL,
    description = true,
    progressBar = true,
    ...attributes
}) => {
    const location = useLocation()
    const [codeHostToDelete, setCodeHostToDelete] = useState<CodeHostToDelete | null>(null)

    const editConnectionRouteMatch = matchPath(`${baseURL}/:codehostId/edit`, location.pathname)
    const newConnectionRouteMatch = matchPath(`${baseURL}/:codeHostType/create`, location.pathname)

    const codeHostQueryResult = useQuery<GetCodeHostsResult>(GET_CODE_HOSTS, {
        fetchPolicy: 'cache-and-network',
        // Polling the most recent data about code host in order to track
        // the current progress of repositories syncing
        pollInterval: 5000,
    })

    useEffect(() => {
        telemetryService.log('SetupWizardLandedAddRemoteCode')
    }, [telemetryService])

    const handleNextButtonClick = (): void => {
        const logEvent = getNextButtonLogEvent(codeHostQueryResult.data)

        if (logEvent) {
            telemetryService.log(logEvent)
        }
    }

    return (
        <div {...attributes} className={classNames(className, styles.root)}>
            {description && <Text className="mb-2">Connect remote code hosts where your source code lives.</Text>}

            <CodeHostExternalServiceAlert />

            <section className={styles.content}>
                <Container className={styles.contentNavigation}>
                    <CodeHostsNavigation
                        codeHostQueryResult={codeHostQueryResult}
                        activeConnectionId={editConnectionRouteMatch?.params?.codehostId}
                        createConnectionType={newConnectionRouteMatch?.params?.codeHostType}
                        className={styles.navigation}
                        onCodeHostDelete={setCodeHostToDelete}
                    />
                </Container>

                <Container className={styles.contentMain}>
                    <Routes>
                        <Route index={true} element={<CodeHostsPicker />} />
                        <Route
                            path=":codeHostType/create"
                            element={<CodeHostCreation telemetryService={telemetryService} />}
                        />
                        <Route
                            path=":codehostId/edit"
                            element={
                                <CodeHostEdit
                                    telemetryService={telemetryService}
                                    onCodeHostDelete={setCodeHostToDelete}
                                />
                            }
                        />
                    </Routes>
                </Container>
            </section>

            {progressBar && (
                <FooterWidget>
                    <ProgressBar />
                </FooterWidget>
            )}

            <CustomNextButton
                label={getNextButtonLabel(codeHostQueryResult.data)}
                disabled={!isAnyConnectedCodeHosts(codeHostQueryResult.data)}
                tooltip={
                    isAnyConnectedCodeHosts(codeHostQueryResult.data)
                        ? 'You can get back to this later'
                        : 'You have to connect at least one code host'
                }
                onClick={handleNextButtonClick}
            />

            {codeHostToDelete && (
                <CodeHostDeleteModal codeHost={codeHostToDelete} onDismiss={() => setCodeHostToDelete(null)} />
            )}
        </div>
    )
}
