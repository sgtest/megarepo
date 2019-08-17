import mkdirp from 'mkdirp-promise'
import * as path from 'path'
import puppeteer from 'puppeteer'

/**
 * Registers a jasmine reporter (for use with jest) that takes a screenshot of the browser when a test fails (and
 * closes the page after each test). It is used by e2e tests.
 *
 * From https://github.com/smooth-code/jest-puppeteer/issues/131#issuecomment-424073620.
 */
export function saveScreenshotsUponFailuresAndClosePage(
    repoRootDir: string,
    screenshotDir: string,
    getPage: () => puppeteer.Page
): void {
    /**
     * jasmine reporter does not support async, so we store the promise and wait for it before each test.
     */
    let promise = Promise.resolve()
    beforeEach(() => promise)
    afterAll(() => promise)

    /**
     * Take a screenshot when a test fails. Jest standard reporters run in a separate process so they don't have
     * access to the page instance. Using jasmine reporter allows us to have access to the test result, test name
     * and page instance at the same time.
     */
    jasmine.getEnv().addReporter({
        specDone: result => {
            if (result.status === 'failed') {
                promise = promise
                    .catch()
                    .then(() => takeScreenshot(getPage(), repoRootDir, screenshotDir, result.fullName))
            }
        },
    })
}

async function takeScreenshot(
    page: puppeteer.Page,
    repoRootDir: string,
    screenshotDir: string,
    testName: string
): Promise<void> {
    await mkdirp(screenshotDir)
    const filePath = path.join(screenshotDir, testName.replace(/\W/g, '_') + '.png')
    await page.screenshot({ path: filePath })
    if (process.env.CI) {
        // Print image with ANSI escape code for Buildkite: https://buildkite.com/docs/builds/images-in-log-output.
        console.log(`\u001B]1338;url="artifact://${path.relative(repoRootDir, filePath)}";alt="Screenshot"\u0007`)
    } else {
        console.log(`Saved screenshot of failure to ${path.relative(process.cwd(), filePath)}`)
    }
}
