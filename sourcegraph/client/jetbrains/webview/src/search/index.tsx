import { render } from 'react-dom'

import { AuthenticatedUser } from '@sourcegraph/shared/src/auth'
import polyfillEventSource from '@sourcegraph/shared/src/polyfills/vendor/eventSource'
import { AnchorLink, setLinkComponent } from '@sourcegraph/wildcard'

import { getAuthenticatedUser } from '../sourcegraph-api-access/api-gateway'
import { EventLogger } from '../telemetry/EventLogger'

import { App } from './App'
import { handleRequest } from './java-to-js-bridge'
import {
    getConfigAlwaysFulfill,
    getThemeAlwaysFulfill,
    indicateFinishedLoading,
    loadLastSearchAlwaysFulfill,
    onOpen,
    onPreviewChange,
    onPreviewClear,
} from './js-to-java-bridge'
import type { PluginConfig, Search, Theme } from './types'

setLinkComponent(AnchorLink)

let isDarkTheme = false
let instanceURL = 'https://sourcegraph.com/'
let isGlobbingEnabled = false
let accessToken: string | null = null
let anonymousUserId: string
let pluginVersion: string
let initialSearch: Search | null = null
let authenticatedUser: AuthenticatedUser | null = null
let telemetryService: EventLogger

window.initializeSourcegraph = async () => {
    const [theme, config, lastSearch]: [Theme, PluginConfig, Search | null] = await Promise.all([
        getThemeAlwaysFulfill(),
        getConfigAlwaysFulfill(),
        loadLastSearchAlwaysFulfill(),
    ])

    applyConfig(config)
    applyTheme(theme)
    applyLastSearch(lastSearch)

    let isServerAccessSuccessful = false
    try {
        authenticatedUser = await getAuthenticatedUser(instanceURL, accessToken)
        isServerAccessSuccessful = true
    } catch (error) {
        console.info('Could not authenticate with current URL and token settings', instanceURL, accessToken, error)
    }

    if (accessToken && !authenticatedUser) {
        console.warn(`No initial authenticated user with access token “${accessToken}”`)
    }

    telemetryService = new EventLogger(anonymousUserId, { editor: 'jetbrains', version: pluginVersion })

    renderReactApp()

    await indicateFinishedLoading(isServerAccessSuccessful, !!authenticatedUser)
}

window.callJS = handleRequest

export function renderReactApp(): void {
    const node = document.querySelector('#main') as HTMLDivElement
    render(
        <App
            // Make sure we recreate the React app when the instance URL or access token changes to
            // avoid showing stale data.
            key={`${instanceURL}-${accessToken}`}
            isDarkTheme={isDarkTheme}
            instanceURL={instanceURL}
            isGlobbingEnabled={isGlobbingEnabled}
            accessToken={accessToken}
            initialSearch={initialSearch}
            onOpen={onOpen}
            onPreviewChange={onPreviewChange}
            onPreviewClear={onPreviewClear}
            authenticatedUser={authenticatedUser}
            telemetryService={telemetryService}
        />,
        node
    )
}

export function applyConfig(config: PluginConfig): void {
    instanceURL = config.instanceURL
    isGlobbingEnabled = config.isGlobbingEnabled || false
    accessToken = config.accessToken || null
    anonymousUserId = config.anonymousUserId || 'no-user-id'
    pluginVersion = config.pluginVersion
    polyfillEventSource(accessToken ? { Authorization: `token ${accessToken}` } : {})
}

export function applyTheme(theme: Theme, rootElement: Element = document.documentElement): void {
    // Dark/light theme
    rootElement.classList.add('theme')
    rootElement.classList.remove(theme.isDarkTheme ? 'theme-light' : 'theme-dark')
    rootElement.classList.add(theme.isDarkTheme ? 'theme-dark' : 'theme-light')
    isDarkTheme = theme.isDarkTheme

    // Find the name of properties here: https://plugins.jetbrains.com/docs/intellij/themes-metadata.html#key-naming-scheme
    const intelliJTheme = theme.intelliJTheme
    const root = document.querySelector(':root') as HTMLElement

    root.style.setProperty('--button-color', intelliJTheme['Button.default.startBackground'])
    root.style.setProperty('--primary', intelliJTheme['Button.default.startBackground'])
    root.style.setProperty('--subtle-bg', intelliJTheme['ScrollPane.background'])

    root.style.setProperty('--dropdown-bg', intelliJTheme['List.background'])
    root.style.setProperty('--dropdown-link-active-bg', intelliJTheme['List.selectionBackground'])
    root.style.setProperty('--dropdown-link-hover-bg', intelliJTheme['ToolbarComboWidget.hoverBackground'])
    root.style.setProperty('--light-text', intelliJTheme['List.selectionForeground'])
    root.style.setProperty('--tooltip-bg', intelliJTheme['ToolTip.background'])
    root.style.setProperty('--tooltip-color', intelliJTheme['ToolTip.foreground'])

    root.style.setProperty('--jb-list-bg', intelliJTheme['List.background'])
    root.style.setProperty('--jb-button-bg', intelliJTheme['Button.startBackground'])
    root.style.setProperty('--jb-text-color', intelliJTheme.text)
    root.style.setProperty('--jb-inactive-text-color', intelliJTheme.textInactiveText)
    root.style.setProperty('--jb-hover-button-bg', intelliJTheme['ActionButton.hoverBackground'])
    root.style.setProperty('--jb-input-bg', intelliJTheme['TextField.background'])
    root.style.setProperty('--jb-selection-bg', intelliJTheme['TextField.selectionBackground'] || '#2675bf')
    root.style.setProperty('--jb-selection-color', intelliJTheme['TextField.selectionForeground'] || '#ffffff')
    root.style.setProperty('--jb-tooltip-bg', intelliJTheme['ToolTip.background'])
    root.style.setProperty('--jb-border-color', intelliJTheme['Component.borderColor'])
    root.style.setProperty('--jb-focus-border-color', intelliJTheme['Component.focusedBorderColor'])
    root.style.setProperty('--jb-icon-color', intelliJTheme['Component.iconColor'] || '#7f8b91')
    root.style.setProperty('--jb-info-text-color', intelliJTheme['Component.infoForeground'])
    root.style.setProperty('--jb-secondary-info-text-color', intelliJTheme['Component.infoForeground'] + '80') // 50% opacity

    // There is no color for this in the serialized theme, so I have picked this option from the
    // Dracula theme
    root.style.setProperty('--code-bg', theme.isDarkTheme ? '#2b2b2b' : '#ffffff')
    root.style.setProperty('--body-bg', intelliJTheme['List.background'])
}

function applyLastSearch(lastSearch: Search | null): void {
    initialSearch = lastSearch
}

export function getAccessToken(): string | null {
    return accessToken
}

export function getInstanceURL(): string {
    return instanceURL
}
