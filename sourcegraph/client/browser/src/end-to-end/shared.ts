import assert from 'assert'

import expect from 'expect'
import puppeteer from 'puppeteer'

import { Driver } from '@sourcegraph/shared/src/testing/driver'
import { retry } from '@sourcegraph/shared/src/testing/utils'
import { createURLWithUTM } from '@sourcegraph/shared/src/tracking/utm'

/**
 * Defines e2e tests for a single-file page of a code host.
 */
export function testSingleFilePage({
    getDriver,
    url,
    sourcegraphBaseUrl,
    repoName,
    getLineSelector,
    goToDefinitionURL,
}: {
    /** Called to get the driver */
    getDriver: () => Driver

    /** The URL to sourcegraph/jsonrpc2 call_opt.go at commit 4fb7cd90793ee6ab445f466b900e6bffb9b63d78 on the code host */
    url: string

    /** The base URL of the sourcegraph instance */
    sourcegraphBaseUrl: string

    /** The repo name of sourcgraph/jsonrpc2 on the Sourcegraph instance */
    repoName: string

    /** The CSS selector for a line (with or without line number part) in the code view */
    getLineSelector: (lineNumber: number) => string

    /** The expected URL for the "Go to Definition" button */
    goToDefinitionURL?: string
}): void {
    describe('File views', () => {
        it('adds "View on Sourcegraph" buttons to files', async () => {
            await getDriver().page.goto(url)

            // Make sure the tab is active, because it might not be active if the install page has opened.
            await closeInstallPageTab(getDriver().page.browser())

            await getDriver().page.waitForSelector(
                '[data-testid="code-view-toolbar"] [data-testid="open-on-sourcegraph"]',
                { timeout: 10000 }
            )
            expect(
                await getDriver().page.$$('[data-testid="code-view-toolbar"] [data-testid="open-on-sourcegraph"]')
            ).toHaveLength(1)

            // TODO: Uncomment this portion of the test once we migrate from puppeteer-firefox to puppeteer
            // We want to assert that Sourcegraph is opened in a new tab, but the old version of Firefox used
            // by puppeteer-firefox doesn't support the latest version of Sourcegraph. For now,
            // simply assert on the link.

            // await getDriver().page.click('[data-testid="code-view-toolbar"] [data-testid="open-on-sourcegraph"]')

            // // The button opens a new tab, so get the new page whose opener is the current page, and get its url.
            // const currentPageTarget = getDriver().page.target()
            // const newTarget = await getDriver().browser.waitForTarget(target => target.opener() === currentPageTarget)
            // const newPage = await newTarget.page()
            // expect(newPage.url()).toBe(
            //     `${sourcegraphBaseUrl}/${repoName}@4fb7cd90793ee6ab445f466b900e6bffb9b63d78/-/blob/call_opt.go?utm_source=chrome-extension`
            // )
            // await newPage.close()

            await retry(async () => {
                assert.strictEqual(
                    await getDriver().page.evaluate(
                        () =>
                            document.querySelector<HTMLAnchorElement>(
                                '[data-testid="code-view-toolbar"] [data-testid="open-on-sourcegraph"]'
                            )?.href
                    ),
                    createURLWithUTM(
                        new URL(
                            `${sourcegraphBaseUrl}/${repoName}@4fb7cd90793ee6ab445f466b900e6bffb9b63d78/-/blob/call_opt.go`
                        ),
                        { utm_source: `${getDriver().browserType}-extension`, utm_campaign: 'open-on-sourcegraph' }
                    ).href
                )
            })
        })

        it('shows hover tooltips when hovering a token', async () => {
            await getDriver().page.goto(url)
            await getDriver().page.waitForSelector(
                '[data-testid="code-view-toolbar"] [data-testid="open-on-sourcegraph"]'
            )

            // Trigger tokenization of the line.
            const lineNumber = 16
            const line = await getDriver().page.waitForSelector(getLineSelector(lineNumber), {
                timeout: 10000,
            })

            if (!line) {
                throw new Error(`Found no line with number ${lineNumber}`)
            }

            // Hover line to give codeintellify time to register listeners for
            // tokenization (only necessary in CI, not sure why).
            await line.hover()

            const [token] = await line.$x('.//span[text()="CallOption"]')
            await token.hover()
            await getDriver().page.waitForSelector('.test-tooltip-go-to-definition')
            await getDriver().page.waitForSelector('.test-tooltip-content')
            await retry(async () => {
                assert.strictEqual(
                    await getDriver().page.evaluate(
                        () => document.querySelector<HTMLAnchorElement>('.test-tooltip-go-to-definition')?.href
                    ),
                    goToDefinitionURL ||
                        `${sourcegraphBaseUrl}/${repoName}@4fb7cd90793ee6ab445f466b900e6bffb9b63d78/-/blob/call_opt.go#L5:6`
                )
            })
        })
    })
}

/**
 * Find a tab that contains the browser extension's after-install page (url
 * ending in `/after_install.html`) and, if found, close it.
 *
 * The after-install page is opened automatically when the browser extension is
 * installed. In tests, this means that it's opened automatically every time we
 * start the browser (with the browser extension loaded).
 */
export async function closeInstallPageTab(browser: puppeteer.Browser): Promise<void> {
    const pages = await browser.pages()
    const installPage = pages.find(page => page.url().endsWith('/after_install.html'))
    if (installPage) {
        await installPage.close()
    }
}
