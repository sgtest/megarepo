import { createDriverForTest } from '../../../shared/src/testing/driver'
import MockDate from 'mockdate'
import { getConfig } from '../../../shared/src/testing/config'
import assert from 'assert'
import expect from 'expect'
import { describeIntegration } from './helpers'
import { commonGraphQlResults } from './graphQlResults'
import { ILanguage, IRepository, IQuery, ISearchResultMatch } from '../../../shared/src/graphql/schema'

const searchResults = {
    data: {
        search: {
            results: {
                __typename: 'SearchResults',
                limitHit: true,
                matchCount: 30,
                approximateResultCount: '30+',
                missing: [] as IRepository[],
                cloning: [] as IRepository[],
                repositoriesCount: 372,
                timedout: [] as IRepository[],
                indexUnavailable: false,
                dynamicFilters: [
                    {
                        value: 'archived:yes',
                        label: 'archived:yes',
                        count: 5,
                        limitHit: true,
                        kind: 'repo',
                    },
                    {
                        value: 'fork:yes',
                        label: 'fork:yes',
                        count: 46,
                        limitHit: true,
                        kind: 'repo',
                    },
                    {
                        value: 'repo:^github\\.com/Algorilla/manta-ray$',
                        label: 'github.com/Algorilla/manta-ray',
                        count: 1,
                        limitHit: false,
                        kind: 'repo',
                    },
                ],
                results: [
                    {
                        __typename: 'Repository',
                        id: 'UmVwb3NpdG9yeTozODcxOTM4Nw==',
                        name: 'github.com/Algorilla/manta-ray',
                        label: {
                            html:
                                '\u003Cp\u003E\u003Ca href="/github.com/Algorilla/manta-ray" rel="nofollow"\u003Egithub.com/Algorilla/manta-ray\u003C/a\u003E\u003C/p\u003E\n',
                        },
                        url: '/github.com/Algorilla/manta-ray',
                        icon:
                            'data:image/svg+xml;base64,PHN2ZyB2ZXJzaW9uPSIxLjEiIGlkPSJMYXllcl8xIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIiB4PSIwcHgiIHk9IjBweCIKCSB2aWV3Qm94PSIwIDAgNjQgNjQiIHN0eWxlPSJlbmFibGUtYmFja2dyb3VuZDpuZXcgMCAwIDY0IDY0OyIgeG1sOnNwYWNlPSJwcmVzZXJ2ZSI+Cjx0aXRsZT5JY29ucyA0MDA8L3RpdGxlPgo8Zz4KCTxwYXRoIGQ9Ik0yMywyMi40YzEuMywwLDIuNC0xLjEsMi40LTIuNHMtMS4xLTIuNC0yLjQtMi40Yy0xLjMsMC0yLjQsMS4xLTIuNCwyLjRTMjEuNywyMi40LDIzLDIyLjR6Ii8+Cgk8cGF0aCBkPSJNMzUsMjYuNGMxLjMsMCwyLjQtMS4xLDIuNC0yLjRzLTEuMS0yLjQtMi40LTIuNHMtMi40LDEuMS0yLjQsMi40UzMzLjcsMjYuNCwzNSwyNi40eiIvPgoJPHBhdGggZD0iTTIzLDQyLjRjMS4zLDAsMi40LTEuMSwyLjQtMi40cy0xLjEtMi40LTIuNC0yLjRzLTIuNCwxLjEtMi40LDIuNFMyMS43LDQyLjQsMjMsNDIuNHoiLz4KCTxwYXRoIGQ9Ik01MCwxNmgtMS41Yy0wLjMsMC0wLjUsMC4yLTAuNSwwLjV2MzVjMCwwLjMtMC4yLDAuNS0wLjUsMC41aC0yN2MtMC41LDAtMS0wLjItMS40LTAuNmwtMC42LTAuNmMtMC4xLTAuMS0wLjEtMC4yLTAuMS0wLjQKCQljMC0wLjMsMC4yLTAuNSwwLjUtMC41SDQ0YzEuMSwwLDItMC45LDItMlYxMmMwLTEuMS0wLjktMi0yLTJIMTRjLTEuMSwwLTIsMC45LTIsMnYzNi4zYzAsMS4xLDAuNCwyLjEsMS4yLDIuOGwzLjEsMy4xCgkJYzEuMSwxLjEsMi43LDEuOCw0LjIsMS44SDUwYzEuMSwwLDItMC45LDItMlYxOEM1MiwxNi45LDUxLjEsMTYsNTAsMTZ6IE0xOSwyMGMwLTIuMiwxLjgtNCw0LTRjMS40LDAsMi44LDAuOCwzLjUsMgoJCWMxLjEsMS45LDAuNCw0LjMtMS41LDUuNFYzM2MxLTAuNiwyLjMtMC45LDQtMC45YzEsMCwyLTAuNSwyLjgtMS4zQzMyLjUsMzAsMzMsMjkuMSwzMywyOHYtMC42Yy0xLjItMC43LTItMi0yLTMuNQoJCWMwLTIuMiwxLjgtNCw0LTRjMi4yLDAsNCwxLjgsNCw0YzAsMS41LTAuOCwyLjctMiwzLjVoMGMtMC4xLDIuMS0wLjksNC40LTIuNSw2Yy0xLjYsMS42LTMuNCwyLjQtNS41LDIuNWMtMC44LDAtMS40LDAuMS0xLjksMC4zCgkJYy0wLjIsMC4xLTEsMC44LTEuMiwwLjlDMjYuNiwzOCwyNywzOC45LDI3LDQwYzAsMi4yLTEuOCw0LTQsNHMtNC0xLjgtNC00YzAtMS41LDAuOC0yLjcsMi0zLjRWMjMuNEMxOS44LDIyLjcsMTksMjEuNCwxOSwyMHoiLz4KPC9nPgo8L3N2Zz4K',
                        detail: { html: '\u003Cp\u003ERepository name match\u003C/p\u003E\n' },
                        matches: [] as ISearchResultMatch[],
                    },
                ],
                alert: null,
                elapsedMilliseconds: 103,
            },
        },
    } as IQuery,
    errors: undefined,
}

describeIntegration('Search', ({ initGeneration, describe }) => {
    initGeneration(async () => {
        // Reset date mocking
        MockDate.reset()
        const { sourcegraphBaseUrl, headless, slowMo, testUserPassword } = getConfig(
            'sourcegraphBaseUrl',
            'headless',
            'slowMo',
            'testUserPassword'
        )

        // Start browser
        const driver = await createDriverForTest({
            sourcegraphBaseUrl,
            logBrowserConsole: true,
            headless,
            slowMo,
        })
        await driver.ensureLoggedIn({ username: 'test', password: testUserPassword, email: 'test@test.com' })
        return { driver, sourcegraphBaseUrl }
    })

    describe('Interactive search mode', ({ test }) => {
        test('Search mode component appears', async ({ sourcegraphBaseUrl, driver }) => {
            await driver.page.goto(sourcegraphBaseUrl + '/search')
            await driver.page.waitForSelector('.e2e-search-mode-toggle')
            expect(await driver.page.evaluate(() => document.querySelectorAll('.e2e-search-mode-toggle').length)).toBe(
                1
            )
        })

        test('Filter buttons', async ({ sourcegraphBaseUrl, driver, overrideGraphQL }) => {
            overrideGraphQL({
                ...commonGraphQlResults,
                SearchSuggestions: {
                    data: {
                        search: {
                            suggestions: [
                                { __typename: 'Language' } as ILanguage,
                                { __typename: 'Repository', name: 'github.com/gorilla/mux' } as IRepository,
                                { __typename: 'Repository', name: 'github.com/gorilla/css' } as IRepository,
                                { __typename: 'Repository', name: 'github.com/gorilla/rpc' } as IRepository,
                            ],
                        },
                    } as IQuery,
                    errors: undefined,
                },
                Search: searchResults,
                RepoGroups: {
                    data: {
                        repoGroups: [
                            { __typename: 'RepoGroup', name: 'go2generics' },
                            { __typename: 'RepoGroup', name: 'city-of-amsterdam' },
                        ],
                    } as IQuery,
                    errors: undefined,
                },
            })
            await driver.page.goto(sourcegraphBaseUrl + '/search')
            await driver.page.waitForSelector('.e2e-search-mode-toggle', { visible: true })
            await driver.page.click('.e2e-search-mode-toggle')
            await driver.page.click('.e2e-search-mode-toggle__interactive-mode')

            // Wait for the input component to appear
            await driver.page.waitForSelector('.e2e-interactive-mode-input', { visible: true })
            // Wait for the add filter row to appear.
            await driver.page.waitForSelector('.e2e-add-filter-row', { visible: true })
            // Wait for the default add filter buttons appear
            await driver.page.waitForSelector('.e2e-add-filter-button-repo', { visible: true })
            await driver.page.waitForSelector('.e2e-add-filter-button-file', { visible: true })

            // Add a repo filter
            await driver.page.waitForSelector('.e2e-add-filter-button-repo')
            await driver.page.click('.e2e-add-filter-button-repo')

            // FilterInput is autofocused
            await driver.page.waitForSelector('.filter-input')
            // Search for repo:gorilla in the repo filter chip input
            await driver.page.keyboard.type('gorilla')
            await driver.page.keyboard.press('Enter')
            await driver.assertWindowLocation('/search?q=repo:%22gorilla%22&patternType=literal')

            // Edit the filter
            await driver.page.waitForSelector('.filter-input')
            await driver.page.click('.filter-input')
            await driver.page.waitForSelector('.filter-input__input-field')
            await driver.page.keyboard.type('/mux')
            // Press enter to lock in filter
            await driver.page.keyboard.press('Enter')
            // The main query input should be autofocused, so hit enter again to submit
            await driver.assertWindowLocation('/search?q=repo:%22gorilla/mux%22&patternType=literal')

            // Add a file filter from search results page
            await driver.page.waitForSelector('.e2e-add-filter-button-file', { visible: true })
            await driver.page.click('.e2e-add-filter-button-file')
            await driver.page.waitForSelector('.filter-input__input-field', { visible: true })
            await driver.page.keyboard.type('README')
            await driver.page.keyboard.press('Enter')
            await driver.page.keyboard.press('Enter')
            await driver.assertWindowLocation('/search?q=repo:%22gorilla/mux%22+file:%22README%22&patternType=literal')

            // Delete filter
            await driver.page.goto(sourcegraphBaseUrl + '/search?q=repo:gorilla/mux&patternType=literal')
            await driver.page.waitForSelector('.e2e-filter-input__delete-button', { visible: true })
            await driver.page.click('.e2e-filter-input__delete-button')
            await driver.assertWindowLocation('/search?q=&patternType=literal')

            // Test suggestions
            await driver.page.goto(sourcegraphBaseUrl + '/search')
            await driver.page.waitForSelector('.e2e-add-filter-button-repo', { visible: true })
            await driver.page.click('.e2e-add-filter-button-repo')
            await driver.page.waitForSelector('.filter-input', { visible: true })
            await driver.page.waitForSelector('.filter-input__input-field')
            await driver.page.keyboard.type('gorilla')
            await driver.page.waitForSelector('.e2e-filter-input__suggestions')
            await driver.page.waitForSelector('.e2e-suggestion-item')
            await driver.page.keyboard.press('ArrowDown')
            await driver.page.keyboard.press('Enter')
            await driver.page.keyboard.press('Enter')
            await driver.assertWindowLocation(
                '/search?q=repo:%22%5Egithub%5C%5C.com/gorilla/mux%24%22&patternType=literal'
            )

            // Test cancelling editing an input with escape key
            await driver.page.click('.filter-input__button-text')
            await driver.page.waitForSelector('.filter-input__input-field')
            await driver.page.keyboard.type('/mux')
            await driver.page.keyboard.press('Escape')
            await driver.page.click('.e2e-search-button')
            await driver.assertWindowLocation(
                '/search?q=repo:%22%5Egithub%5C%5C.com/gorilla/mux%24%22&patternType=literal'
            )

            // Test cancelling editing an input by clicking outside close button
            await driver.page.click('.filter-input__button-text')
            await driver.page.waitForSelector('.filter-input__input-field')
            await driver.page.keyboard.type('/mux')
            await driver.page.click('.e2e-search-button')
            await driver.assertWindowLocation(
                '/search?q=repo:%22%5Egithub%5C%5C.com/gorilla/mux%24%22&patternType=literal'
            )
        })

        test('Updates query when searching from directory page', async ({
            sourcegraphBaseUrl,
            driver,
            overrideGraphQL,
        }) => {
            overrideGraphQL({
                ...commonGraphQlResults,
                RepositoryRedirect: {
                    data: {
                        repositoryRedirect: {
                            __typename: 'Repository',
                            id: 'SourcegraphJsonRpc2RepositoryID',
                            name: 'github.com/sourcegraph/jsonrpc2',
                            url: '/github.com/sourcegraph/jsonrpc2',
                            externalURLs: [{ url: 'https://github.com/sourcegraph/jsonrpc2', serviceType: 'github' }],
                            description:
                                'Package jsonrpc2 provides a client and server implementation of JSON-RPC 2.0 (http://www.jsonrpc.org/specification)',
                            viewerCanAdminister: true,
                            defaultBranch: { displayName: 'master' },
                        },
                    } as IQuery,
                    errors: undefined,
                },
                ResolveRev: {
                    data: {
                        repositoryRedirect: {
                            __typename: 'Repository',
                            mirrorInfo: { cloneInProgress: false, cloneProgress: '', cloned: true },
                            commit: {
                                oid: '15c2290dcb37731cc4ee5a2a1c1e5a25b4c28f81',
                                tree: { url: '/github.com/sourcegraph/jsonrpc2' },
                            },
                            defaultBranch: { abbrevName: 'master' },
                        },
                    } as IQuery,
                    errors: undefined,
                },
            })
            await driver.page.goto(sourcegraphBaseUrl + '/github.com/sourcegraph/jsonrpc2')
            await driver.page.waitForSelector('.filter-input')
            const filterInputValue = () =>
                driver.page.evaluate(() => {
                    const filterInput = document.querySelector<HTMLButtonElement>('.filter-input__button-text')
                    return filterInput ? filterInput.textContent : null
                })
            assert.strictEqual(await filterInputValue(), 'repo:^github\\.com/sourcegraph/jsonrpc2$')
        })

        test('Filter dropdown and finite-option filter inputs', async ({
            sourcegraphBaseUrl,
            driver,
            overrideGraphQL,
        }) => {
            overrideGraphQL({
                ...commonGraphQlResults,
                Search: searchResults,
            })
            await driver.page.goto(sourcegraphBaseUrl + '/search')
            await driver.page.waitForSelector('.e2e-query-input', { visible: true })
            await driver.page.waitForSelector('.e2e-filter-dropdown')
            await driver.page.type('.e2e-query-input', 'test')
            await driver.page.click('.e2e-filter-dropdown')
            await driver.page.select('.e2e-filter-dropdown', 'fork')
            await driver.page.waitForSelector('.e2e-filter-input-finite-form')
            await driver.page.waitForSelector('.e2e-filter-input-radio-button-no')
            await driver.page.click('.e2e-filter-input-radio-button-no')
            await driver.page.click('.e2e-confirm-filter-button')
            await driver.assertWindowLocation('/search?q=fork:%22no%22+test&patternType=literal')
            // Edit filter
            await driver.page.waitForSelector('.filter-input')
            await driver.page.waitForSelector('.e2e-filter-input__button-text-fork')
            await driver.page.click('.e2e-filter-input__button-text-fork')
            await driver.page.waitForSelector('.e2e-filter-input-radio-button-only')
            await driver.page.click('.e2e-filter-input-radio-button-only')
            await driver.page.click('.e2e-confirm-filter-button')
            await driver.assertWindowLocation('/search?q=fork:%22only%22+test&patternType=literal')
            // Edit filter by clicking dropdown menu
            await driver.page.waitForSelector('.e2e-filter-dropdown')
            await driver.page.click('.e2e-filter-dropdown')
            await driver.page.select('.e2e-filter-dropdown', 'fork')
            await driver.page.waitForSelector('.e2e-filter-input-finite-form')
            await driver.page.waitForSelector('.e2e-filter-input-radio-button-no')
            await driver.page.click('.e2e-filter-input-radio-button-no')
            await driver.page.click('.e2e-confirm-filter-button')
            await driver.assertWindowLocation('/search?q=fork:%22no%22+test&patternType=literal')
        })
    })

    describe('Case sensitivity toggle', ({ test }) => {
        test('Clicking toggle turns on case sensitivity', async ({ sourcegraphBaseUrl, driver }) => {
            await driver.page.goto(sourcegraphBaseUrl + '/search')
            await driver.page.waitForSelector('.e2e-query-input', { visible: true })
            await driver.page.waitForSelector('.e2e-case-sensitivity-toggle')
            await driver.page.type('.e2e-query-input', 'test')
            await driver.page.click('.e2e-case-sensitivity-toggle')
            await driver.assertWindowLocation('/search?q=test&patternType=literal&case=yes')
        })

        test('Clicking toggle turns off case sensitivity and removes case= URL parameter', async ({
            sourcegraphBaseUrl,
            driver,
        }) => {
            await driver.page.goto(sourcegraphBaseUrl + '/search?q=test&patternType=literal&case=yes')
            await driver.page.waitForSelector('.e2e-query-input', { visible: true })
            await driver.page.waitForSelector('.e2e-case-sensitivity-toggle')
            await driver.page.click('.e2e-case-sensitivity-toggle')
            await driver.assertWindowLocation('/search?q=test&patternType=literal')
        })
    })

    describe('Structural search toggle', ({ test }) => {
        test('Clicking toggle turns on structural search', async ({ sourcegraphBaseUrl, driver }) => {
            await driver.page.goto(sourcegraphBaseUrl + '/search')
            await driver.page.waitForSelector('.e2e-query-input', { visible: true })
            await driver.page.waitForSelector('.e2e-structural-search-toggle')
            await driver.page.type('.e2e-query-input', 'test')
            await driver.page.click('.e2e-structural-search-toggle')
            await driver.assertWindowLocation('/search?q=test&patternType=structural')
        })

        test('Clicking toggle turns on structural search and removes existing patternType parameter', async ({
            sourcegraphBaseUrl,
            driver,
        }) => {
            await driver.page.goto(sourcegraphBaseUrl + '/search?q=test&patternType=regexp')
            await driver.page.waitForSelector('.e2e-query-input', { visible: true })
            await driver.page.waitForSelector('.e2e-structural-search-toggle')
            await driver.page.click('.e2e-structural-search-toggle')
            await driver.assertWindowLocation('/search?q=test&patternType=structural')
        })

        test('Clicking toggle turns off structural saerch and reverts to default pattern type', async ({
            sourcegraphBaseUrl,
            driver,
        }) => {
            await driver.page.goto(sourcegraphBaseUrl + '/search?q=test&patternType=structural')
            await driver.page.waitForSelector('.e2e-query-input', { visible: true })
            await driver.page.waitForSelector('.e2e-structural-search-toggle')
            await driver.page.click('.e2e-structural-search-toggle')
            await driver.assertWindowLocation('/search?q=test&patternType=literal')
        })
    })
})
