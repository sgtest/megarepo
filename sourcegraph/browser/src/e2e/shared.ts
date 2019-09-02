import { Driver } from '../../../shared/src/e2e/driver'

/**
 * Defines e2e tests for a single-file page of a code host.
 */
export function testSingleFilePage({
    getDriver,
    url,
    sourcegraphBaseUrl,
    repoName,
    lineSelector,
}: {
    /** Called to get the driver */
    getDriver: () => Driver

    /** The URL to sourcegraph/jsonrpc2 call_opt.go at commit 4fb7cd90793ee6ab445f466b900e6bffb9b63d78 on the code host */
    url: string

    /** The base URL of the sourcegraph instance */
    sourcegraphBaseUrl: string

    /** The repo name of sourcgraph/jsonrpc2 on the Sourcegraph instance */
    repoName: string

    /** The CSS selector for a line in the code view */
    lineSelector: string
}): void {
    it('adds "View on Sourcegraph" buttons to files', async () => {
        await getDriver().page.goto(url)
        await getDriver().page.waitForSelector('.code-view-toolbar .open-on-sourcegraph', { timeout: 10000 })
        expect(await getDriver().page.$$('.code-view-toolbar .open-on-sourcegraph')).toHaveLength(1)
        await Promise.all([
            getDriver().page.waitForNavigation(),
            getDriver().page.click('.code-view-toolbar .open-on-sourcegraph'),
        ])
        expect(getDriver().page.url()).toBe(
            `${sourcegraphBaseUrl}/${repoName}@4fb7cd90793ee6ab445f466b900e6bffb9b63d78/-/blob/call_opt.go`
        )
    })

    it('shows hover tooltips when hovering a token', async () => {
        await getDriver().page.goto(url)
        await getDriver().page.waitForSelector('.code-view-toolbar .open-on-sourcegraph')

        // Pause to give codeintellify time to register listeners for
        // tokenization (only necessary in CI, not sure why).
        await getDriver().page.waitFor(1000)

        // Trigger tokenization of the line.
        const lineNumber = 16
        const line = await getDriver().page.waitForSelector(`${lineSelector}:nth-child(${lineNumber})`, {
            timeout: 10000,
        })
        const [token] = await line.$x('//span[text()="CallOption"]')
        await token.hover()
        await getDriver().page.waitForSelector('.e2e-tooltip-go-to-definition')
        await Promise.all([
            getDriver().page.waitForNavigation(),
            getDriver().page.click('.e2e-tooltip-go-to-definition'),
        ])
        expect(await getDriver().page.evaluate(() => location.href)).toBe(
            `${sourcegraphBaseUrl}/${repoName}@4fb7cd90793ee6ab445f466b900e6bffb9b63d78/-/blob/call_opt.go#L5:6`
        )
    })
}
