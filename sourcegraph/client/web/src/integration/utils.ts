import { Page } from 'puppeteer'

import { SharedGraphQlOperations } from '@sourcegraph/shared/src/graphql-operations'
import { percySnapshot } from '@sourcegraph/shared/src/testing/driver'
import { readEnvironmentBoolean } from '@sourcegraph/shared/src/testing/utils'

import { WebGraphQlOperations } from '../graphql-operations'

const CODE_HIGHLIGHTING_QUERIES: Partial<keyof (WebGraphQlOperations & SharedGraphQlOperations)>[] = [
    'highlightCode',
    'Blob',
]

/**
 * Matches a URL against an expected query that will handle code highlighting.
 */
const isCodeHighlightQuery = (url: string): boolean =>
    CODE_HIGHLIGHTING_QUERIES.some(query => url.includes(`graphql?${query}`))

/**
 * Watches and waits for requests to complete that match expected code highlighting queries.
 * Useful to ensure styles are fully loaded after changing the webapp color scheme.
 */
const waitForCodeHighlighting = async (page: Page): Promise<void> => {
    const requestDidFire = await page
        .waitForRequest(request => isCodeHighlightQuery(request.url()), { timeout: 1000 })
        .catch(
            () =>
                // request won't always fire if data is cached
                false
        )

    if (requestDidFire) {
        await page.waitForResponse(request => isCodeHighlightQuery(request.url()), { timeout: 1000 })
    }
}

type ColorScheme = 'dark' | 'light'

const ColorSchemeToMonacoEditorClassName: Record<ColorScheme, string> = {
    dark: 'vs-dark',
    light: 'vs',
}

/**
 * Update all theme styling on the Sourcegraph webapp to match a color scheme.
 */
export const setColorScheme = async (
    page: Page,
    scheme: ColorScheme,
    shouldWaitForCodeHighlighting?: boolean
): Promise<void> => {
    const isAlreadySet = await page.evaluate(
        (scheme: ColorScheme) => matchMedia(`(prefers-color-scheme: ${scheme})`).matches,
        scheme
    )

    if (isAlreadySet) {
        return
    }

    await Promise.all([
        page.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: scheme }]),
        shouldWaitForCodeHighlighting ? waitForCodeHighlighting(page) : Promise.resolve(),
    ])

    // Check Monaco editor is styled correctly
    await page.waitForFunction(
        expectedClassName => document.querySelector('.monaco-editor')?.classList.contains(expectedClassName),
        { timeout: 1000 },
        ColorSchemeToMonacoEditorClassName[scheme]
    )
}

export interface PercySnapshotConfig {
    waitForCodeHighlighting: boolean
}

/**
 * Takes a Percy snapshot in 4 variants:
 * dark/dark-redesign/light/light-redesign
 */
export const percySnapshotWithVariants = async (
    page: Page,
    name: string,
    config?: PercySnapshotConfig
): Promise<void> => {
    const percyEnabled = readEnvironmentBoolean({ variable: 'PERCY_ON', defaultValue: false })

    if (!percyEnabled) {
        return
    }

    // Theme-light
    await setColorScheme(page, 'light', config?.waitForCodeHighlighting)
    await percySnapshot(page, `${name} - light theme`)

    // Theme-light with redesign
    await page.evaluate(() => document.documentElement.classList.add('theme-redesign'))
    await percySnapshot(page, `${name} - light theme with redesign enabled`)
    await page.evaluate(() => document.documentElement.classList.remove('theme-redesign'))

    // Theme-dark
    await setColorScheme(page, 'dark', config?.waitForCodeHighlighting)
    await percySnapshot(page, `${name} - dark theme`)

    // Theme-dark with redesign
    await page.evaluate(() => document.documentElement.classList.add('theme-redesign'))
    await percySnapshot(page, `${name} - dark theme with redesign enabled`)
    await page.evaluate(() => document.documentElement.classList.remove('theme-redesign'))

    // Reset to light theme
    await setColorScheme(page, 'light', config?.waitForCodeHighlighting)
}
