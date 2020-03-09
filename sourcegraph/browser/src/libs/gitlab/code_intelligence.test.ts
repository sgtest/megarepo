import {
    testCodeHostMountGetters as testMountGetters,
    testToolbarMountGetter,
} from '../code_intelligence/code_intelligence_test_utils'
import { getToolbarMount, gitlabCodeHost } from './code_intelligence'
import { readFile } from 'mz/fs'

describe('gitlab/code_intelligence', () => {
    describe('gitlabCodeHost', () => {
        testMountGetters(gitlabCodeHost, `${__dirname}/__fixtures__/repository.html`)
    })
    describe('getToolbarMount()', () => {
        testToolbarMountGetter(`${__dirname}/__fixtures__/code-views/merge-request/unified.html`, getToolbarMount)
    })

    describe('urlToFile()', () => {
        const { urlToFile } = gitlabCodeHost
        const sourcegraphURL = 'https://sourcegraph.my.org'

        beforeAll(async () => {
            document.documentElement.innerHTML = await readFile(__dirname + '/__fixtures__/merge-request.html', 'utf-8')
            jsdom.reconfigure({ url: 'https://gitlab.com/sourcegraph/jsonrpc2/merge_requests/1/diffs' })
            globalThis.gon = { gitlab_url: 'https://gitlab.com' }
        })
        it('returns an URL to the Sourcegraph instance if the location has a viewState', () => {
            expect(
                urlToFile(
                    sourcegraphURL,
                    {
                        repoName: 'sourcegraph/sourcegraph',
                        rawRepoName: 'gitlab.com/sourcegraph/sourcegraph',
                        rev: 'master',
                        filePath: 'browser/src/libs/code_intelligence/code_intelligence.tsx',
                        position: {
                            line: 5,
                            character: 12,
                        },
                        viewState: 'references',
                    },
                    { part: undefined }
                )
            ).toBe(
                'https://sourcegraph.my.org/sourcegraph/sourcegraph@master/-/blob/browser/src/libs/code_intelligence/code_intelligence.tsx#L5:12&tab=references'
            )
        })

        it('returns an absolute URL if the location is not on the same code host', () => {
            expect(
                urlToFile(
                    sourcegraphURL,
                    {
                        repoName: 'sourcegraph/sourcegraph',
                        rawRepoName: 'gitlab.sgdev.org/sourcegraph/sourcegraph',
                        rev: 'master',
                        filePath: 'browser/src/libs/code_intelligence/code_intelligence.tsx',
                        position: {
                            line: 5,
                            character: 12,
                        },
                    },
                    { part: undefined }
                )
            ).toBe(
                'https://sourcegraph.my.org/sourcegraph/sourcegraph@master/-/blob/browser/src/libs/code_intelligence/code_intelligence.tsx#L5:12'
            )
        })
        it('returns an URL to a blob on the same code host if possible', () => {
            expect(
                urlToFile(
                    sourcegraphURL,
                    {
                        repoName: 'sourcegraph/sourcegraph',
                        rawRepoName: 'gitlab.com/sourcegraph/sourcegraph',
                        rev: 'master',
                        filePath: 'browser/src/libs/code_intelligence/code_intelligence.tsx',
                        position: {
                            line: 5,
                            character: 12,
                        },
                    },
                    { part: undefined }
                )
            ).toBe(
                'https://gitlab.com/sourcegraph/sourcegraph/blob/master/browser/src/libs/code_intelligence/code_intelligence.tsx#L5'
            )
        })
        it('returns an URL to the file on the same merge request if possible', () => {
            expect(
                urlToFile(
                    sourcegraphURL,
                    {
                        repoName: 'sourcegraph/jsonrpc2',
                        rawRepoName: 'gitlab.com/sourcegraph/jsonrpc2',
                        rev: 'changes',
                        filePath: 'call_opt.go',
                        position: {
                            line: 5,
                            character: 12,
                        },
                    },
                    { part: 'head' }
                )
            ).toBe(
                'https://gitlab.com/sourcegraph/jsonrpc2/merge_requests/1/diffs#9e1d3828a925c1eca74b74c20b58a9138f886d29_3_5'
            )
        })
    })
})
