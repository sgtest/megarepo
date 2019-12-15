import '../../../../shared/src/polyfills'

import { setLinkComponent, AnchorLink } from '../../../../shared/src/components/Link'
import { injectCodeIntelligence } from '../code_intelligence/inject'
import { injectExtensionMarker } from '../sourcegraph/inject'
import { getPhabricatorCSS, getSourcegraphURLFromConduit } from './backend'
import { metaClickOverride } from './util'
import { getAssetsURL } from '../../shared/util/context'

// Just for informational purposes (see getPlatformContext())
window.SOURCEGRAPH_PHABRICATOR_EXTENSION = true

const IS_EXTENSION = false

setLinkComponent(AnchorLink)

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

    const sourcegraphURL =
        window.localStorage.getItem('SOURCEGRAPH_URL') ||
        window.SOURCEGRAPH_URL ||
        (await getSourcegraphURLFromConduit())
    const assetsURL = getAssetsURL(sourcegraphURL)

    // Backwards compat: Support Legacy Phabricator extension. Check that the Phabricator integration
    // passed the bundle url. Legacy Phabricator extensions inject CSS via the loader.js script
    // so we do not need to do this here.
    if (!window.SOURCEGRAPH_BUNDLE_URL && !window.localStorage.getItem('SOURCEGRAPH_BUNDLE_URL')) {
        injectExtensionMarker()
        await injectCodeIntelligence({ sourcegraphURL, assetsURL }, IS_EXTENSION)
        metaClickOverride()
        return
    }

    // eslint-disable-next-line require-atomic-updates
    window.SOURCEGRAPH_URL = sourcegraphURL
    const css = await getPhabricatorCSS(sourcegraphURL)
    const style = document.createElement('style')
    style.setAttribute('type', 'text/css')
    style.id = 'sourcegraph-styles'
    style.textContent = css
    document.head.appendChild(style)
    window.localStorage.setItem('SOURCEGRAPH_URL', sourcegraphURL)
    metaClickOverride()
    injectExtensionMarker()
    await injectCodeIntelligence({ sourcegraphURL, assetsURL }, IS_EXTENSION)
}

init().catch(err => console.error('Error initializing Phabricator integration', err))
