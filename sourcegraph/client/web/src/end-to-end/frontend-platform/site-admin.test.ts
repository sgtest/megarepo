import { describe, test, before, after } from 'mocha'

import { getConfig } from '@sourcegraph/shared/src/testing/config'
import { afterEachRecordCoverage } from '@sourcegraph/shared/src/testing/coverage'
import { Driver } from '@sourcegraph/shared/src/testing/driver'
import { afterEachSaveScreenshotIfFailed } from '@sourcegraph/shared/src/testing/screenshotReporter'

import { cloneRepos } from '../utils/cloneRepos'
import { initEndToEndTest } from '../utils/initEndToEndTest'

const { sourcegraphBaseUrl } = getConfig('gitHubDotComToken', 'sourcegraphBaseUrl')

describe('Site Admin', () => {
    let driver: Driver

    before(async function () {
        driver = await initEndToEndTest()

        await cloneRepos({
            driver,
            mochaContext: this,
            repoSlugs: ['gorilla/mux'],
        })
    })

    after('Close browser', () => driver?.close())

    afterEachSaveScreenshotIfFailed(() => driver.page)
    afterEachRecordCoverage(() => driver)

    // Flaky https://github.com/sourcegraph/sourcegraph/issues/45531
    test.skip('Overview', async () => {
        await driver.page.goto(sourcegraphBaseUrl + '/site-admin')
        await driver.page.waitForSelector('[data-testid="product-certificate"', { visible: true })
    })

    test('Repositories list', async () => {
        await driver.page.goto(sourcegraphBaseUrl + '/site-admin/repositories?query=gorilla%2Fmux')
        await driver.page.waitForSelector('a[href="/github.com/gorilla/mux"]', { visible: true })
    })
})
