import '../../config/polyfill'

import * as H from 'history'
import React from 'react'
import { fromEvent, Subscription } from 'rxjs'
import { first } from 'rxjs/operators'
import { setLinkComponent } from '../../../../shared/src/components/Link'
import { storage } from '../../browser/storage'
import { injectCodeIntelligence } from '../../libs/code_intelligence/inject'
import { initSentry } from '../../libs/sentry'
import {
    checkIsSourcegraph,
    EXTENSION_MARKER_ID,
    injectExtensionMarker,
    NATIVE_INTEGRATION_ACTIVATED,
    signalBrowserExtensionInstalled,
} from '../../libs/sourcegraph/inject'
import { DEFAULT_SOURCEGRAPH_URL } from '../../shared/util/context'
import { featureFlags } from '../../shared/util/featureFlags'
import { assertEnv } from '../envAssertion'

const subscriptions = new Subscription()
window.addEventListener('unload', () => subscriptions.unsubscribe(), { once: true })

assertEnv('CONTENT')

initSentry('content')

setLinkComponent(({ to, children, ...props }) => (
    <a href={to && typeof to !== 'string' ? H.createPath(to) : to} {...props}>
        {children}
    </a>
))

const IS_EXTENSION = true

/**
 * Main entry point into browser extension.
 */
async function main(): Promise<void> {
    console.log('Sourcegraph browser extension is running')

    // Make sure DOM is fully loaded
    if (document.readyState !== 'complete' && document.readyState !== 'interactive') {
        await new Promise<Event>(resolve => document.addEventListener('DOMContentLoaded', resolve, { once: true }))
    }

    // Allow users to set this via the console.
    ;(window as any).sourcegraphFeatureFlags = featureFlags

    // Check if a native integration is already running on the page,
    // and abort execution if it's the case.
    // If the native integration was activated before the content script, we can
    // synchronously check for the presence of the extension marker.
    if (document.getElementById(EXTENSION_MARKER_ID) !== null) {
        console.log('Sourcegraph native integration is already running')
        return
    }
    // If the extension marker isn't present, inject it and listen for a custom event sent by the native
    // integration to signal its activation.
    injectExtensionMarker()
    const nativeIntegrationActivationEventReceived = fromEvent(document, NATIVE_INTEGRATION_ACTIVATED)
        .pipe(first())
        .toPromise()

    const items = await storage.sync.get()
    if (items.disableExtension) {
        return
    }

    const sourcegraphServerUrl = items.sourcegraphURL || DEFAULT_SOURCEGRAPH_URL

    const isSourcegraphServer = checkIsSourcegraph(sourcegraphServerUrl)
    if (isSourcegraphServer) {
        signalBrowserExtensionInstalled()
        return
    }

    // Add style sheet and wait for it to load to avoid rendering unstyled elements (which causes an
    // annoying flash/jitter when the stylesheet loads shortly thereafter).
    let styleSheet = document.getElementById('ext-style-sheet') as HTMLLinkElement | null
    // If does not exist, create
    if (!styleSheet) {
        styleSheet = document.createElement('link')
        styleSheet.id = 'ext-style-sheet'
        styleSheet.rel = 'stylesheet'
        styleSheet.type = 'text/css'
        styleSheet.href = browser.extension.getURL('css/style.bundle.css')
    }
    // If not loaded yet, wait for it to load
    if (!styleSheet.sheet) {
        await new Promise(resolve => {
            styleSheet!.addEventListener('load', resolve, { once: true })
            // If not appended yet, append to <head>
            if (!styleSheet!.parentNode) {
                document.head.appendChild(styleSheet!)
            }
        })
    }

    subscriptions.add(await injectCodeIntelligence(IS_EXTENSION))

    // Clean up susbscription if the native integration gets activated
    // later in the lifetime of the content script.
    await nativeIntegrationActivationEventReceived
    console.log('Native integration activation event received')
    subscriptions.unsubscribe()
}

main().catch(console.error.bind(console))
