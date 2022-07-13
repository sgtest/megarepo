import { Page } from 'puppeteer'

import { SearchGraphQlOperations } from '@sourcegraph/search'
import { SharedGraphQlOperations } from '@sourcegraph/shared/src/graphql-operations'
import { Settings, SettingsExperimentalFeatures } from '@sourcegraph/shared/src/schema/settings.schema'
import { Driver, percySnapshot } from '@sourcegraph/shared/src/testing/driver'
import { readEnvironmentBoolean } from '@sourcegraph/shared/src/testing/utils'

import { WebGraphQlOperations } from '../graphql-operations'

const CODE_HIGHLIGHTING_QUERIES: Partial<
    keyof (WebGraphQlOperations & SharedGraphQlOperations & SearchGraphQlOperations)
>[] = ['highlightCode', 'Blob', 'HighlightedFile']

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

/**
 * Percy couldn't capture <img /> since they have `src` values with testing domain name.
 * We need to call this function before asking Percy to take snapshots,
 * <img /> with base64 data would be visible on Percy snapshot
 */
export const convertImgSourceHttpToBase64 = async (page: Page): Promise<void> => {
    await page.evaluate(() => {
        // Skip images with data-skip-percy
        // Skip images with .cm-widgetBuffer, which CodeMirror uses when using a widget decoration
        // See https://github.com/sourcegraph/sourcegraph/issues/28949
        const imgs = document.querySelectorAll<HTMLImageElement>('img:not([data-skip-percy]):not(.cm-widgetBuffer)')

        for (const img of imgs) {
            if (img.src.startsWith('data:image')) {
                continue
            }

            const canvas = document.createElement('canvas')
            canvas.width = img.width
            canvas.height = img.height

            const context = canvas.getContext('2d')
            context?.drawImage(img, 0, 0)

            img.src = canvas.toDataURL('image/png')
        }
    })
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
}

export interface PercySnapshotConfig {
    /**
     * How long to wait for the UI to settle before taking a screenshot.
     */
    timeout: number
    waitForCodeHighlighting: boolean
}

/**
 * Takes a Percy snapshot in 2 variants: dark/light
 */
export const percySnapshotWithVariants = async (
    page: Page,
    name: string,
    { timeout = 1000, waitForCodeHighlighting = false } = {}
): Promise<void> => {
    const percyEnabled = readEnvironmentBoolean({ variable: 'PERCY_ON', defaultValue: false })

    if (!percyEnabled) {
        return
    }

    // Theme-dark
    await setColorScheme(page, 'dark', waitForCodeHighlighting)
    // Wait for the UI to settle before converting images and taking the
    // screenshot.
    await page.waitForTimeout(timeout)
    await convertImgSourceHttpToBase64(page)
    await percySnapshot(page, `${name} - dark theme`)

    // Theme-light
    await setColorScheme(page, 'light', waitForCodeHighlighting)
    // Wait for the UI to settle before converting images and taking the
    // screenshot.
    await page.waitForTimeout(timeout)
    await convertImgSourceHttpToBase64(page)
    await percySnapshot(page, `${name} - light theme`)
}

type Editor = NonNullable<SettingsExperimentalFeatures['editor']>

export interface EditorAPI {
    name: Editor
    /**
     * Wait for editor to appear in the DOM.
     */
    waitForIt: (options?: Parameters<Page['waitForSelector']>[1]) => Promise<void>
    /**
     * Wait for suggestion with provided label appears
     */
    waitForSuggestion: (label?: string) => Promise<void>
    /**
     * Moves focus to the editor's root node.
     */
    focus: () => Promise<void>
    /**
     * Returns the current value of the editor.
     */
    getValue: () => Promise<string | null | undefined>
    /**
     * Replaces the editor's content with the provided input.
     */
    replace: (input: string, method?: 'type' | 'paste') => Promise<void>
    /**
     * Triggers application of the specified suggestion.
     */
    selectSuggestion: (label: string) => Promise<void>
}

const editors: Record<Editor, (driver: Driver, rootSelector: string) => EditorAPI> = {
    monaco: (driver: Driver, rootSelector: string) => {
        const inputSelector = `${rootSelector} textarea`
        // Selector to use to wait for the editor to be complete loaded
        const readySelector = `${rootSelector} .view-lines`
        const completionSelector = `${rootSelector} .suggest-widget.visible`
        const completionLabelSelector = `${completionSelector} span`

        const api: EditorAPI = {
            name: 'monaco',
            async waitForIt(options) {
                await driver.page.waitForSelector(readySelector, options)
            },
            async focus() {
                await api.waitForIt()
                await driver.page.click(rootSelector)
            },
            getValue() {
                return driver.page.evaluate(
                    (inputSelector: string) => document.querySelector<HTMLTextAreaElement>(inputSelector)?.value,
                    inputSelector
                )
            },
            replace(newText: string, method = 'type') {
                return driver.replaceText({
                    selector: rootSelector,
                    newText,
                    enterTextMethod: method,
                    selectMethod: 'keyboard',
                })
            },
            async waitForSuggestion(suggestion?: string) {
                await driver.page.waitForSelector(completionSelector)
                if (suggestion !== undefined) {
                    await driver.findElementWithText(suggestion, {
                        selector: completionLabelSelector,
                        wait: { timeout: 5000 },
                    })
                }
            },
            async selectSuggestion(suggestion: string) {
                await driver.page.waitForSelector(completionSelector)
                await driver.findElementWithText(suggestion, {
                    action: 'click',
                    selector: completionLabelSelector,
                    wait: { timeout: 5000 },
                })
            },
        }
        return api
    },
    codemirror6: (driver: Driver, rootSelector: string) => {
        const inputSelector = `${rootSelector} .cm-content`
        // Selector to use to wait for the editor to be complete loaded
        const readySelector = `${rootSelector} .cm-line`
        const completionSelector = `${rootSelector} .cm-tooltip-autocomplete`
        const completionLabelSelector = `${completionSelector} .cm-completionLabel`

        const api: EditorAPI = {
            name: 'codemirror6',
            async waitForIt(options) {
                await driver.page.waitForSelector(readySelector, options)
            },
            async focus() {
                await api.waitForIt()
                await driver.page.click(rootSelector)
            },
            getValue() {
                return driver.page.evaluate(
                    (inputSelector: string) => document.querySelector<HTMLDivElement>(inputSelector)?.textContent,
                    inputSelector
                )
            },
            replace(newText: string, method = 'type') {
                return driver.replaceText({
                    selector: rootSelector,
                    newText,
                    enterTextMethod: method,
                    selectMethod: 'selectall',
                })
            },
            async waitForSuggestion(suggestion?: string) {
                await driver.page.waitForSelector(completionSelector)
                if (suggestion !== undefined) {
                    await driver.findElementWithText(suggestion, {
                        selector: completionLabelSelector,
                        wait: { timeout: 5000 },
                    })
                }
                // It seems CodeMirror needs some additional time before it
                // recognizes events on the suggestions element (such as
                // selecting a suggestion via the Tab key)
                await driver.page.waitForTimeout(100)
            },
            async selectSuggestion(suggestion: string) {
                await driver.page.waitForSelector(completionSelector)
                await driver.findElementWithText(suggestion, {
                    action: 'click',
                    selector: completionLabelSelector,
                    wait: { timeout: 5000 },
                })
            },
        }
        return api
    },
}

/**
 * Creates the necessary user settings mock for enabling the specified editor.
 * The caller is responsible for mocking the response with the returned object.
 */
export function enableEditor(editor: Editor): Partial<Settings> {
    return {
        experimentalFeatures: {
            editor,
        },
    }
}

/**
 * Returns an object for accessing editor related information at `rootSelector`.
 * It also waits for the editor to be ready
 */
export const createEditorAPI = async (driver: Driver, rootSelector: string): Promise<EditorAPI> => {
    await driver.page.waitForSelector(rootSelector)
    const editor = await driver.page.evaluate<(selector: string) => string | undefined>(
        selector => (document.querySelector(selector) as HTMLElement).dataset.editor,
        rootSelector
    )
    switch (editor) {
        case 'monaco':
        case 'codemirror6':
            break
        default:
            throw new Error(`${editor} is not a supported editor`)
    }
    const api = editors[editor](driver, rootSelector)
    await api.waitForIt()
    return api
}

/**
 * Helper function for abstracting away testing different search query input
 * implementations. The callback function gets passed the editor name which can
 * be used with {@link enableEditor} and {@link createEditorAPI}.
 */
export const withSearchQueryInput = (callback: (editorName: Editor) => void): void => {
    const editorNames: Editor[] = ['monaco', 'codemirror6']
    for (const editor of editorNames) {
        // This callback is supposed to be called multiple times
        // eslint-disable-next-line callback-return
        callback(editor)
    }
}
