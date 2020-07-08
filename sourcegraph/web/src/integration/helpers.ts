import { describe as mochaDescribe, test as mochaTest, before as mochaBefore } from 'mocha'
import { Subscription, Subject, throwError } from 'rxjs'
import { snakeCase } from 'lodash'
import { createDriverForTest, Driver } from '../../../shared/src/testing/driver'
import mkdirp from 'mkdirp-promise'
import express from 'express'
import { Polly } from '@pollyjs/core'
import { PuppeteerAdapter } from './polly/PuppeteerAdapter'
import FSPersister from '@pollyjs/persister-fs'
import { ErrorGraphQLResult, SuccessGraphQLResult } from '../../../shared/src/graphql/graphql'
import { IMutation, IQuery } from '../../../shared/src/graphql/schema'
import { first, timeoutWith } from 'rxjs/operators'
import * as path from 'path'
import * as util from 'util'
import { commonGraphQlResults } from './graphQlResults'
import * as prettier from 'prettier'

// Reduce log verbosity
util.inspect.defaultOptions.depth = 0
util.inspect.defaultOptions.maxStringLength = 80

Polly.register(PuppeteerAdapter as any)
Polly.register(FSPersister)

const FIXTURES_DIRECTORY = `${__dirname}/__fixtures__`
const ASSETS_DIRECTORY = `${__dirname}/../../../ui/assets`

type IntegrationTestInitGeneration = () => Promise<{
    driver: Driver
    sourcegraphBaseUrl: string
    subscriptions?: Subscription
}>

export type GraphQLOverrides = Record<string, SuccessGraphQLResult<IQuery | IMutation> | ErrorGraphQLResult>

interface TestContext {
    sourcegraphBaseUrl: string
    driver: Driver

    /**
     * Configures fake responses for GraphQL queries and mutations.
     *
     * @param overrides The results to return, keyed by query name.
     */
    overrideGraphQL: (overrides: GraphQLOverrides) => void

    /**
     * Waits for a specific GraphQL query to happen and returns the variables passed to the request.
     * If the query does not happen within a few seconds, it throws a timeout error.
     *
     * @param triggerRequest A callback called to trigger the request (e.g. clicking a button). The request MUST be triggered within this callback.
     * @param queryName The name of the query to wait for.
     * @returns The GraphQL variables of the query.
     */
    waitForGraphQLRequest: (triggerRequest: () => Promise<void> | void, queryName: string) => Promise<unknown>
}

type TestBody = (context: TestContext) => Promise<void>

interface IntegrationTestDefiner {
    (title: string, run: TestBody): void
    only: (title: string, run: TestBody) => void
    skip: (title: string, run?: TestBody) => void
}

type IntegrationTestBeforeGeneration = (
    setupLogic: (options: { sourcegraphBaseUrl: string; driver: Driver }) => Promise<void>
) => void

type IntegrationTestDescriber = (
    title: string,
    suite: (helpers: {
        before: IntegrationTestBeforeGeneration
        test: IntegrationTestDefiner
        it: IntegrationTestDefiner
        describe: IntegrationTestDescriber
    }) => void
) => void

type IntegrationTestSuite = (helpers: {
    /**
     * Registers a calback that will be run before test data is generated,
     * responsible for creating the {@link Driver} and providing the Sourcegraph URL.
     */
    initGeneration: (setupLogic: IntegrationTestInitGeneration) => void
    test: IntegrationTestDefiner
    describe: IntegrationTestDescriber
}) => void

/**
 * Describes an integration test suite using wrappers over Mocha primitives.
 *
 * To record test data, set the RECORD environment variable to a truthy value.
 * When recording, the tests will be run in Puppeteer against a real backend,
 * and captured response fixtures will be saved to the `__fixtures__` directory.
 *
 * When running the tests, static CSS/JS assets will be served from the `ui/assets` directory.
 * Other requests (for instance to the GraphQL API) will be stubbed using response
 * stubs from the `__fixtures__` directory, through Puppeteer's request interception.
 *
 */
export function describeIntegration(description: string, testSuite: IntegrationTestSuite): void {
    const record = Boolean(process.env.RECORD)
    mochaDescribe(description, () => {
        let driver: Driver
        let sourcegraphBaseUrl: string
        const subscriptions = new Subscription()

        const test = (prefixes: string[]): IntegrationTestDefiner => {
            const wrapTestBody = (title: string, run: TestBody) => async () => {
                await driver.newPage()
                await driver.page.setRequestInterception(true)
                const recordingsDirectory = path.join(FIXTURES_DIRECTORY, ...prefixes.map(snakeCase))
                if (record) {
                    await mkdirp(recordingsDirectory)
                }
                const polly = new Polly(snakeCase(title), {
                    adapters: ['puppeteer'],
                    adapterOptions: {
                        puppeteer: { page: driver.page, requestResourceTypes: ['xhr', 'fetch', 'document'] },
                    },
                    persister: 'fs',
                    persisterOptions: {
                        fs: {
                            recordingsDir: recordingsDirectory,
                        },
                    },
                    expiryStrategy: 'warn',
                    recordIfMissing: record,
                    matchRequestsBy: {
                        method: true,
                        body: true,
                        order: true,
                        // Origin header will change when running against a test instance
                        headers: false,
                        url: {
                            pathname: true,
                            query: true,
                            hash: true,
                            // Allow recording tests against https://sourcegraph.test
                            // but running them against http:://localhost:8000
                            protocol: false,
                            port: false,
                            hostname: false,
                            username: false,
                            password: false,
                        },
                    },
                    mode: record ? 'record' : 'replay',
                    logging: false,
                })
                const { server } = polly
                server.get('/.assets/*path').passthrough()

                const errors = new Subject<never>()

                // GraphQL requests are not handled by HARs, but configured per-test.
                let graphQlOverrides: GraphQLOverrides = commonGraphQlResults
                const graphQlRequests = new Subject<{ queryName: string; variables: unknown }>()
                server.post(new URL('/.api/graphql', sourcegraphBaseUrl).href).intercept((request, response) => {
                    const queryName = new URL(request.absoluteUrl).search.slice(1)
                    const { variables, query } = request.jsonBody() as { query: string; variables: string }
                    graphQlRequests.next({ queryName, variables })
                    if (!graphQlOverrides || !Object.prototype.hasOwnProperty.call(graphQlOverrides, queryName)) {
                        const formattedQuery = prettier.format(query, { parser: 'graphql' }).trim()
                        const formattedVariables = util.inspect(variables)
                        const error = new Error(
                            `GraphQL query "${queryName}" has no configured mock response. Make sure the call to overrideGraphQL() includes a result for the "${queryName}" query:\n${formattedVariables} ⤵️\n${formattedQuery}`
                        )
                        // Make test fail
                        errors.error(error)
                        throw error
                    }
                    const result = graphQlOverrides[queryName]
                    response.json(result)
                })

                // Filter out 'server' header filled in by Caddy before persisting responses,
                // otherwise tests will hang when replayed from recordings.
                server
                    .any()
                    .on(
                        'beforePersist',
                        (request, recording: { response: { headers: { name: string; value: string }[] } }) => {
                            recording.response.headers = recording.response.headers.filter(
                                ({ name }) => name !== 'server'
                            )
                        }
                    )
                try {
                    await Promise.race([
                        errors.toPromise(),
                        run({
                            sourcegraphBaseUrl,
                            driver,
                            overrideGraphQL: overrides => {
                                graphQlOverrides = overrides
                            },
                            waitForGraphQLRequest: async (triggerRequest, queryName) => {
                                const requestPromise = graphQlRequests
                                    .pipe(
                                        first(request => request.queryName === queryName),
                                        timeoutWith(
                                            4000,
                                            throwError(new Error(`Timeout waiting for GraphQL request "${queryName}"`))
                                        )
                                    )
                                    .toPromise()
                                await triggerRequest()
                                const { variables } = await requestPromise
                                return variables
                            },
                        }),
                    ])
                } finally {
                    await polly.stop()
                    await driver.page.close()
                }
            }
            return Object.assign(
                (title: string, run: TestBody) => {
                    mochaTest(title, wrapTestBody(title, run))
                },
                {
                    only: (title: string, run: TestBody) => {
                        mochaTest.only(title, wrapTestBody(title, run))
                    },
                    skip: (title: string) => {
                        mochaTest.skip(title)
                    },
                }
            )
        }
        const before: IntegrationTestBeforeGeneration = setupLogic => {
            if (record) {
                mochaBefore(() => setupLogic({ driver, sourcegraphBaseUrl }))
            }
        }
        const describe = (prefixes: string[]): IntegrationTestDescriber => (title, suite) => {
            mochaDescribe(title, () => {
                suite({
                    describe: describe([...prefixes, title]),
                    before,
                    it: test([...prefixes, title]),
                    test: test([...prefixes, title]),
                })
            })
        }
        let initGenerationCallback: IntegrationTestInitGeneration | null = null
        const initGeneration = (logic: IntegrationTestInitGeneration): void => {
            initGenerationCallback = logic
        }
        testSuite({ initGeneration, test: test([description]), describe: describe([description]) })
        mochaBefore(async () => {
            if (!initGenerationCallback) {
                throw new Error('initGeneration() was never called')
            }
            if (record) {
                const setupResult = await initGenerationCallback()
                driver = setupResult.driver
                sourcegraphBaseUrl = setupResult.sourcegraphBaseUrl
                if (setupResult.subscriptions) {
                    subscriptions.add(setupResult.subscriptions)
                }
            } else {
                sourcegraphBaseUrl = 'http://localhost:8000'
                driver = await createDriverForTest({
                    sourcegraphBaseUrl,
                    logBrowserConsole: false,
                })
                // Serve static assets from `ui/assets`
                const app = express()
                app.use('/.assets', express.static(ASSETS_DIRECTORY))
                const server = app.listen(8000)
                subscriptions.add(() => server.close())
            }
        })
        after(async () => {
            await driver?.close()
            // eslint-disable-next-line no-unused-expressions
            subscriptions?.unsubscribe()
        })
    })
}
