import '../../config/polyfill'

import * as H from 'history'
import React from 'react'
import { Observable, Subscription } from 'rxjs'
import { startWith } from 'rxjs/operators'
import { setLinkComponent } from '../../../../shared/src/components/Link'
import { storage } from '../../browser/storage'
import { determineCodeHost as detectCodeHost, injectCodeIntelligenceToCodeHost } from '../../libs/code_intelligence'
import { initSentry } from '../../libs/sentry'
import { checkIsSourcegraph, injectSourcegraphApp } from '../../libs/sourcegraph/inject'
import { DEFAULT_SOURCEGRAPH_URL } from '../../shared/util/context'
import { MutationRecordLike, observeMutations } from '../../shared/util/dom'
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

    // This is checked for in the webapp
    const extensionMarker = document.createElement('div')
    extensionMarker.id = 'sourcegraph-app-background'
    extensionMarker.style.display = 'none'
    if (document.getElementById(extensionMarker.id)) {
        return
    }

    const mutations: Observable<MutationRecordLike[]> = observeMutations(document.body, {
        childList: true,
        subtree: true,
    }).pipe(startWith([{ addedNodes: [document.body], removedNodes: [] }]))

    const items = await storage.sync.get()
    if (items.disableExtension) {
        return
    }

    const sourcegraphServerUrl = items.sourcegraphURL || DEFAULT_SOURCEGRAPH_URL

    const isSourcegraphServer = checkIsSourcegraph(sourcegraphServerUrl)

    // Check which code host we are on
    const codeHost = detectCodeHost()
    if (!codeHost && !isSourcegraphServer) {
        return
    }

    // Add style sheet and wait for it to load to avoid rendering unstyled elements (which causes an
    // annoying flash/jitter when the stylesheet loads shortly thereafter).
    if (!isSourcegraphServer) {
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
    }

    // Add a marker to the DOM that the extension is loaded
    injectSourcegraphApp(extensionMarker)

    // For the life time of the content script, add features in reaction to DOM changes
    if (codeHost) {
        console.log('Detected code host', codeHost.type)
        subscriptions.add(await injectCodeIntelligenceToCodeHost(mutations, codeHost, IS_EXTENSION))
    }
}

main().catch(console.error.bind(console))
