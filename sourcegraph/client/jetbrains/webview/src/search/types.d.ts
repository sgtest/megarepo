import type { SearchPatternType } from '@sourcegraph/search'

import type { Request } from './js-to-java-bridge'

/* Add global functions to global window object */
declare global {
    interface Window {
        initializeSourcegraph: () => Promise<void>
        callJava: (request: Request) => Promise<object>
    }
}

export interface Theme {
    isDarkTheme: boolean
    buttonColor: string
}

export interface PluginConfig {
    instanceURL: string
    isGlobbingEnabled: boolean
    accessToken: string | null
}

export interface Search {
    query: string | null
    caseSensitive: boolean
    patternType: SearchPatternType
    selectedSearchContextSpec: string
}
