import { useEffect, useRef, useState } from 'react'

import * as jsonc from 'jsonc-parser'
import { useSearchParams } from 'react-router-dom'

import { Container, ErrorAlert, Text, Link } from '@sourcegraph/wildcard'

import { tauriInvoke } from '../../app/tauriIcpUtils'
import { Page } from '../../components/Page'
import { PageTitle } from '../../components/PageTitle'
import { fetchSite, updateSiteConfiguration } from '../../site-admin/backend'
import { eventLogger } from '../../tracking/eventLogger'

export const AppAuthCallbackPage: React.FC = () => {
    useEffect(() => eventLogger.logPageView('AppAuthCallbackPage'), [])

    const [error, setError] = useState<Error | string | null>(null)

    const [searchParams] = useSearchParams()
    const code = searchParams.get('code')
    const destination = searchParams.get('destination')

    const isInvalidUrl = !code || code === ''

    const didSaveRef = useRef(false)
    useEffect(() => {
        if (isInvalidUrl) {
            return
        }
        if (didSaveRef.current) {
            return
        }
        didSaveRef.current = true

        saveAccessToken(code, destination).catch(setError)
    }, [code, isInvalidUrl, destination])

    return (
        <Page>
            <PageTitle title="Auth callback" />
            {isInvalidUrl || error ? (
                <ErrorAlert error={isInvalidUrl ? 'Invalid redirect URL. Please try again.' : error} />
            ) : (
                <Container>
                    <Text>Thank you for connecting your Sourcegraph.com account.</Text>

                    <Text>
                        If you are not redirected shortly, <Link to={destination ?? '/'}>continue here</Link>.
                    </Text>
                </Container>
            )}
        </Page>
    )
}

const defaultModificationOptions: jsonc.ModificationOptions = {
    formattingOptions: {
        eol: '\n',
        insertSpaces: true,
        tabSize: 2,
    },
}

async function saveAccessToken(accessToken: string, destination: string | null): Promise<void> {
    const site = await fetchSite().toPromise()

    const content = site.configuration.effectiveContents
    const id = site.configuration.id

    const modification = jsonc.modify(content, ['app', 'dotcomAuthToken'], accessToken, defaultModificationOptions)
    const modifiedContent = jsonc.applyEdits(content, modification)

    await updateSiteConfiguration(id, modifiedContent).toPromise()

    // If the Cody window is open, we need to reload it so it gets the new site config
    tauriInvoke('reload_cody_window')
    // Also reload the main window so it gets the new site config, and redirect to the destination
    location.href = destination ?? '/'
}
