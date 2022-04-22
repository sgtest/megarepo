import { downloadAndUnzipVSCode } from '@vscode/test-electron'

import { mixedSearchStreamEvents, highlightFileResult } from '@sourcegraph/search'
import { Settings } from '@sourcegraph/shared/src/settings/settings'
import { setupExtensionMocking } from '@sourcegraph/shared/src/testing/integration/mockExtension'

import { createVSCodeIntegrationTestContext, VSCodeIntegrationTestContext } from './context'
import { getVSCodeWebviewFrames } from './getWebview'
import { launchVsCode, VSCodeTestDriver } from './launch'

const sourcegraphBaseUrl = 'https://sourcegraph.com'

describe('VS Code extension', () => {
    let vsCodeDriver: VSCodeTestDriver
    before(async () => {
        const vscodeExecutablePath = await downloadAndUnzipVSCode()
        vsCodeDriver = await launchVsCode(vscodeExecutablePath)
    })
    after(() => vsCodeDriver?.dispose())

    let testContext: VSCodeIntegrationTestContext

    beforeEach(async function () {
        testContext = await createVSCodeIntegrationTestContext(
            {
                currentTest: this.currentTest!,
                directory: __dirname,
            },
            vsCodeDriver.page
        )
    })

    afterEach(async () => {
        // Close Remote File
        await vsCodeDriver.page.waitForSelector('.tabs-container .active .tab-actions', { visible: true })
        await vsCodeDriver.page.click('.tabs-container .active .tab-actions', { delay: 50 })
        // Close Search Panel
        await vsCodeDriver.page.waitForSelector('.tabs-container .active .tab-actions', { visible: true })
        await vsCodeDriver.page.click('.tabs-container .active .tab-actions', { delay: 50 })
    })

    // Debt: reset VS Code extension state between test cases in `afterEach` once we
    // have multiple tests. This will likely involve just closing the search panel.
    // afterEach(async () => {})

    it('works', async () => {
        const { Extensions } = setupExtensionMocking({
            pollyServer: testContext.server,
            sourcegraphBaseUrl,
        })

        const userSettings: Settings = {
            extensions: {},
        }

        testContext.overrideGraphQL({
            Extensions,

            ...highlightFileResult,
            ViewerSettings: () => ({
                viewerSettings: {
                    __typename: 'SettingsCascade',
                    final: JSON.stringify(userSettings),
                    subjects: [
                        {
                            __typename: 'User',
                            displayName: 'Test User',
                            id: 'TestUserSettingsID',
                            latestSettings: {
                                id: 123,
                                contents: JSON.stringify(userSettings),
                            },
                            username: 'test',
                            viewerCanAdminister: true,
                            settingsURL: '/users/test/settings',
                        },
                    ],
                },
            }),
            BlobContent: () => ({
                repository: {
                    commit: {
                        blob: {
                            content: 'testing\rvsce\n',
                            binary: false,
                            byteSize: 2,
                        },
                    },
                },
            }),
            FileNames: () => ({
                repository: {
                    commit: {
                        fileNames: ['overridable/bool_or_string_test.go'],
                    },
                },
            }),
        })

        testContext.overrideSearchStreamEvents([...mixedSearchStreamEvents])

        const { searchPanelFrame, sidebarFrame } = await getVSCodeWebviewFrames(vsCodeDriver.page)

        // Focus search box
        await searchPanelFrame.waitForSelector('.monaco-editor .view-lines')
        await searchPanelFrame.click('.monaco-editor .view-lines')

        await vsCodeDriver.page.keyboard.type('test', {
            delay: 50,
        })
        // Submit search
        await searchPanelFrame.waitForSelector('.test-search-button', { visible: true })
        await searchPanelFrame.click('.test-search-button', { delay: 50 })

        try {
            await searchPanelFrame.waitForSelector('.test-search-result', { visible: true })
        } catch {
            throw new Error('Timeout waiting for search results to render')
        }

        // Submit new search from sidebar filter
        try {
            await sidebarFrame.waitForSelector('.search-sidebar .search-filter-keyword', { visible: true })
            await sidebarFrame.click('.search-sidebar .search-filter-keyword', { delay: 50 })
            await searchPanelFrame.waitForSelector('.test-search-result', { visible: true })
        } catch {
            throw new Error('Timeout waiting for filtered search results to render')
        }

        // Open Repo page from search results
        await searchPanelFrame.waitForSelector('.test-search-result button', { visible: true })
        await searchPanelFrame.click('.test-search-result button', { delay: 50 })

        // Redirect back to search results from Repo Page
        try {
            await searchPanelFrame.waitForSelector('.test-back-to-search-view-btn', { visible: true })
            await searchPanelFrame.click('.test-back-to-search-view-btn', { delay: 50 })
        } catch {
            throw new Error('Timeout waiting for search results to render after viewing repo page')
        }

        // Open remote file from search results
        try {
            await searchPanelFrame.waitForSelector('.test-search-result strong', { visible: true })
            await searchPanelFrame.click('.test-search-result strong', { delay: 100 })
        } catch {
            throw new Error('Timeout waiting for search results to render after nevigating back from repo display page')
        }

        // Look for file title
        const remoteFileTitle = await vsCodeDriver.page.title()
        if (!remoteFileTitle.includes('bool_or_string_test.go')) {
            throw new Error('Timeout waiting for remote file to render')
        }
    })

    // Potential future test cases:
    // - Sourcegraph extensions work on remote files
})
