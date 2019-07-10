import '../../config/polyfill'

import * as H from 'history'
import React from 'react'
import { setLinkComponent } from '../../../../shared/src/components/Link'
import { injectCodeIntelligence } from '../code_intelligence/inject'
import { injectExtensionMarker } from '../sourcegraph/inject'
import { getPhabricatorCSS, getSourcegraphURLFromConduit } from './backend'
import { metaClickOverride } from './util'

// Just for informational purposes (see getPlatformContext())
window.SOURCEGRAPH_PHABRICATOR_EXTENSION = true

const IS_EXTENSION = false

setLinkComponent(({ to, children, ...props }) => (
    <a href={to && typeof to !== 'string' ? H.createPath(to) : to} {...props}>
        {children}
    </a>
))

async function init(): Promise<void> {
    /**
     * This is the main entry point for the phabricator in-page JavaScript plugin.
     */
    if (window.localStorage && window.localStorage.getItem('SOURCEGRAPH_DISABLED') === 'true') {
        console.log(
            `Sourcegraph on Phabricator is disabled because window.localStorage.getItem('SOURCEGRAPH_DISABLED') is set to ${window.localStorage.getItem(
                'SOURCEGRAPH_DISABLED'
            )}.`
        )
        return
    }
    // Backwards compat: Support Legacy Phabricator extension. Check that the Phabricator integration
    // passed the bundle url. Legacy Phabricator extensions inject CSS via the loader.js script
    // so we do not need to do this here.
    if (!window.SOURCEGRAPH_BUNDLE_URL && !window.localStorage.getItem('SOURCEGRAPH_BUNDLE_URL')) {
        injectExtensionMarker()
        await injectCodeIntelligence(IS_EXTENSION)
        metaClickOverride()
        return
    }

    const sourcegraphURL = await getSourcegraphURLFromConduit()
    const css = await getPhabricatorCSS(sourcegraphURL)
    const style = document.createElement('style')
    style.setAttribute('type', 'text/css')
    style.id = 'sourcegraph-styles'
    style.textContent = css
    document.head.appendChild(style)
    window.localStorage.setItem('SOURCEGRAPH_URL', sourcegraphURL)
    window.SOURCEGRAPH_URL = sourcegraphURL
    metaClickOverride()
    injectExtensionMarker()
    await injectCodeIntelligence(IS_EXTENSION)
}

init().catch(err => console.error(`Error initializing Phabricator integration`, err))
