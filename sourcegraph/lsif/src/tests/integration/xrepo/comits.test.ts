import * as util from '../integration-test-util'
import nock from 'nock'
import { XrepoDatabase } from '../../../shared/xrepo/xrepo'

describe('discoverAndUpdateCommit', () => {
    it('should update tracked commits', async () => {
        const ca = util.createCommit('a')
        const cb = util.createCommit('b')
        const cc = util.createCommit('c')

        nock('http://gitserver1')
            .post('/exec')
            .reply(200, `${ca}\n${cb} ${ca}\n${cc} ${cb}`)

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const xrepoDatabase = new XrepoDatabase('', connection)
            await xrepoDatabase.insertDump('test-repo', ca, '')

            await xrepoDatabase.discoverAndUpdateCommit({
                repository: 'test-repo', // hashes to gitserver1
                commit: cc,
                gitserverUrls: ['gitserver0', 'gitserver1', 'gitserver2'],
                ctx: {},
            })

            // Ensure all commits are now tracked
            expect(await xrepoDatabase.isCommitTracked('test-repo', ca)).toBeTruthy()
            expect(await xrepoDatabase.isCommitTracked('test-repo', cb)).toBeTruthy()
            expect(await xrepoDatabase.isCommitTracked('test-repo', cc)).toBeTruthy()
        } finally {
            await cleanup()
        }
    })

    it('should early-out if commit is tracked', async () => {
        const ca = util.createCommit('a')
        const cb = util.createCommit('b')

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const xrepoDatabase = new XrepoDatabase('', connection)
            await xrepoDatabase.insertDump('test-repo', ca, '')
            await xrepoDatabase.updateCommits('test-repo', [[cb, '']])

            // This test ensures the following does not make a gitserver request.
            // As we did not register a nock interceptor, any request will result
            // in an exception being thrown.

            await xrepoDatabase.discoverAndUpdateCommit({
                repository: 'test-repo', // hashes to gitserver1
                commit: cb,
                gitserverUrls: ['gitserver0', 'gitserver1', 'gitserver2'],
                ctx: {},
            })
        } finally {
            await cleanup()
        }
    })

    it('should early-out if repository is unknown', async () => {
        const ca = util.createCommit('a')

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const xrepoDatabase = new XrepoDatabase('', connection)

            // This test ensures the following does not make a gitserver request.
            // As we did not register a nock interceptor, any request will result
            // in an exception being thrown.

            await xrepoDatabase.discoverAndUpdateCommit({
                repository: 'test-repo', // hashes to gitserver1
                commit: ca,
                gitserverUrls: ['gitserver0', 'gitserver1', 'gitserver2'],
                ctx: {},
            })
        } finally {
            await cleanup()
        }
    })
})

describe('discoverAndUpdateTips', () => {
    it('should update tips', async () => {
        const ca = util.createCommit('a')
        const cb = util.createCommit('b')
        const cc = util.createCommit('c')
        const cd = util.createCommit('d')
        const ce = util.createCommit('e')

        nock('http://gitserver0')
            .post('/exec', { repo: 'test-repo', args: ['git', 'rev-parse', 'HEAD'] })
            .reply(200, ce)

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const xrepoDatabase = new XrepoDatabase('', connection)
            await xrepoDatabase.updateCommits('test-repo', [[ca, ''], [cb, ca], [cc, cb], [cd, cc], [ce, cd]])
            await xrepoDatabase.insertDump('test-repo', ca, 'foo')
            await xrepoDatabase.insertDump('test-repo', cb, 'foo')
            await xrepoDatabase.insertDump('test-repo', cc, 'bar')

            await xrepoDatabase.discoverAndUpdateTips({
                gitserverUrls: ['gitserver0'],
                ctx: {},
            })

            const d1 = await xrepoDatabase.getDump('test-repo', ca, 'foo/test.ts')
            const d2 = await xrepoDatabase.getDump('test-repo', cb, 'foo/test.ts')
            const d3 = await xrepoDatabase.getDump('test-repo', cc, 'bar/test.ts')

            expect(d1 && d1.visibleAtTip).toBeFalsy()
            expect(d2 && d2.visibleAtTip).toBeTruthy()
            expect(d3 && d3.visibleAtTip).toBeTruthy()
        } finally {
            await cleanup()
        }
    })
})

describe('discoverTips', () => {
    it('should route requests to correct gitserver', async () => {
        // Distribution of repository names to gitservers
        const requests = {
            'http://gitserver0': [1, 4, 5, 9, 10, 11, 13],
            'http://gitserver1': [0, 3, 6, 7, 12, 14],
            'http://gitserver2': [2, 8],
        }

        // Setup gitsever responses
        for (const [addr, suffixes] of Object.entries(requests)) {
            for (const i of suffixes) {
                nock(addr)
                    .post('/exec', { repo: `test-repo-${i}`, args: ['git', 'rev-parse', 'HEAD'] })
                    .reply(200, `c${i}`)
            }
        }

        // Map repo to the payloads above
        const expected = new Map<string, string>()
        for (let i = 0; i < 15; i++) {
            expected.set(`test-repo-${i}`, `c${i}`)
        }

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const xrepoDatabase = new XrepoDatabase('', connection)

            for (let i = 0; i < 15; i++) {
                await xrepoDatabase.insertDump(`test-repo-${i}`, util.createCommit('c'), '')
            }

            const tips = await xrepoDatabase.discoverTips({
                gitserverUrls: ['gitserver0', 'gitserver1', 'gitserver2'],
                ctx: {},
                batchSize: 5,
            })

            expect(tips).toEqual(expected)
        } finally {
            await cleanup()
        }
    })
})
