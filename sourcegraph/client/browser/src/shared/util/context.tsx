import * as runtime from '../../browser/runtime'
import storage from '../../browser/storage'
import { isPhabricator, isPublicCodeHost } from '../../context'
import { EventLogger } from '../tracking/EventLogger'

export const DEFAULT_SOURCEGRAPH_URL = 'https://sourcegraph.com'

export let eventLogger = new EventLogger()

export let sourcegraphUrl =
    window.localStorage.getItem('SOURCEGRAPH_URL') || window.SOURCEGRAPH_URL || DEFAULT_SOURCEGRAPH_URL

export let inlineSymbolSearchEnabled = false

interface UrlCache {
    [key: string]: string
}

export const repoUrlCache: UrlCache = {}

if (window.SG_ENV === 'EXTENSION') {
    storage.getSync(items => {
        sourcegraphUrl = items.sourcegraphURL
        inlineSymbolSearchEnabled = items.featureFlags.inlineSymbolSearchEnabled
    })
}

export function setSourcegraphUrl(url: string): void {
    sourcegraphUrl = url
}

export function isSourcegraphDotCom(url: string = sourcegraphUrl): boolean {
    return url === DEFAULT_SOURCEGRAPH_URL
}

export function setInlineSymbolSearchEnabled(enabled: boolean): void {
    inlineSymbolSearchEnabled = enabled
}

export function getPlatformName(): 'phabricator-integration' | 'firefox-extension' | 'chrome-extension' {
    if (window.SOURCEGRAPH_PHABRICATOR_EXTENSION) {
        return 'phabricator-integration'
    }

    return isFirefoxExtension() ? 'firefox-extension' : 'chrome-extension'
}

export function getExtensionVersionSync(): string {
    return runtime.getExtensionVersionSync()
}

function isFirefoxExtension(): boolean {
    return window.navigator.userAgent.indexOf('Firefox') !== -1
}

/**
 * Check the DOM to see if we can determine if a repository is private or public.
 */
export function isPrivateRepository(): boolean {
    if (isPhabricator) {
        return true
    }
    if (!isPublicCodeHost) {
        return true
    }
    // @TODO(lguychard) this is github-specific and should not be in /shared
    const header = document.querySelector('.repohead-details-container')
    if (!header) {
        return false
    }
    return !!header.querySelector('.private')
}
