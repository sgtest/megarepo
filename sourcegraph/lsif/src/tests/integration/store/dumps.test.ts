import * as util from '../integration-test-util'
import * as pgModels from '../../../shared/models/pg'
import nock from 'nock'
import rmfr from 'rmfr'
import { Connection } from 'typeorm'
import { DumpManager } from '../../../shared/store/dumps'
import { fail } from 'assert'
import { MAX_TRAVERSAL_LIMIT } from '../../../shared/constants'
import { pick } from 'lodash'

describe('DumpManager', () => {
    let connection!: Connection
    let cleanup!: () => Promise<void>
    let storageRoot!: string
    let dumpManager!: DumpManager

    let counter = 100
    const nextId = () => {
        counter++
        return counter
    }

    beforeAll(async () => {
        ;({ connection, cleanup } = await util.createCleanPostgresDatabase())
        storageRoot = await util.createStorageRoot()
        dumpManager = new DumpManager(connection, storageRoot)
    })

    afterAll(async () => {
        await rmfr(storageRoot)

        if (cleanup) {
            await cleanup()
        }
    })

    beforeEach(async () => {
        if (connection) {
            await util.truncatePostgresTables(connection)
        }
    })

    it('should find closest commits with LSIF data', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This database has the following commit graph:
        //
        // [a] --+--- b --------+--e -- f --+-- [g]
        //       |              |           |
        //       +-- [c] -- d --+           +--- h

        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()
        const cd = util.createCommit()
        const ce = util.createCommit()
        const cf = util.createCommit()
        const cg = util.createCommit()
        const ch = util.createCommit()

        // Add relations
        await dumpManager.updateCommits(
            repositoryId,
            new Map<string, Set<string>>([
                [ca, new Set()],
                [cb, new Set([ca])],
                [cc, new Set([ca])],
                [cd, new Set([cc])],
                [ce, new Set([cb])],
                [ce, new Set([cd])],
                [cf, new Set([ce])],
                [cg, new Set([cf])],
                [ch, new Set([cf])],
            ])
        )

        // Add dumps
        await util.insertDump(connection, dumpManager, repositoryId, ca, '')
        await util.insertDump(connection, dumpManager, repositoryId, cc, '')
        await util.insertDump(connection, dumpManager, repositoryId, cg, '')

        const d1 = await dumpManager.findClosestDump(repositoryId, ca, 'file')
        const d2 = await dumpManager.findClosestDump(repositoryId, cb, 'file')
        const d3 = await dumpManager.findClosestDump(repositoryId, cc, 'file')
        const d4 = await dumpManager.findClosestDump(repositoryId, cd, 'file')
        const d5 = await dumpManager.findClosestDump(repositoryId, cf, 'file')
        const d6 = await dumpManager.findClosestDump(repositoryId, cg, 'file')
        const d7 = await dumpManager.findClosestDump(repositoryId, ce, 'file')
        const d8 = await dumpManager.findClosestDump(repositoryId, ch, 'file')

        // Test closest commit
        expect(d1?.commit).toEqual(ca)
        expect(d2?.commit).toEqual(ca)
        expect(d3?.commit).toEqual(cc)
        expect(d4?.commit).toEqual(cc)
        expect(d5?.commit).toEqual(cg)
        expect(d6?.commit).toEqual(cg)

        // Multiple nearest are chosen arbitrarily
        expect([ca, cc, cg]).toContain(d7?.commit)
        expect([ca, cc]).toContain(d8?.commit)
    })

    it('should return empty string as closest commit with no reachable lsif data', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This database has the following commit graph:
        //
        // a --+-- [b] ---- c
        //     |
        //     +--- d --+-- e -- f
        //              |
        //              +-- g -- h

        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()
        const cd = util.createCommit()
        const ce = util.createCommit()
        const cf = util.createCommit()
        const cg = util.createCommit()
        const ch = util.createCommit()

        // Add relations
        await dumpManager.updateCommits(
            repositoryId,
            new Map<string, Set<string>>([
                [ca, new Set()],
                [cb, new Set([ca])],
                [cc, new Set([cb])],
                [cd, new Set([ca])],
                [ce, new Set([cd])],
                [cf, new Set([ce])],
                [cg, new Set([cd])],
                [ch, new Set([cg])],
            ])
        )

        // Add dumps
        await util.insertDump(connection, dumpManager, repositoryId, cb, '')

        const d1 = await dumpManager.findClosestDump(repositoryId, ca, 'file')
        const d2 = await dumpManager.findClosestDump(repositoryId, cb, 'file')
        const d3 = await dumpManager.findClosestDump(repositoryId, cc, 'file')

        // Test closest commit
        expect(d1?.commit).toEqual(cb)
        expect(d2?.commit).toEqual(cb)
        expect(d3?.commit).toEqual(cb)
        expect(await dumpManager.findClosestDump(repositoryId, cd, 'file')).toBeUndefined()
        expect(await dumpManager.findClosestDump(repositoryId, ce, 'file')).toBeUndefined()
        expect(await dumpManager.findClosestDump(repositoryId, cf, 'file')).toBeUndefined()
        expect(await dumpManager.findClosestDump(repositoryId, cg, 'file')).toBeUndefined()
        expect(await dumpManager.findClosestDump(repositoryId, ch, 'file')).toBeUndefined()
    })

    it('should return empty string as closest commit with no reachable lsif data', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This database has the following commit graph:
        //
        // a --+-- [b]
        //
        // Where LSIF dumps exist at b at roots: root1/ and root2/.

        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const fields = ['repositoryId', 'commit', 'root']

        // Add relations
        await dumpManager.updateCommits(
            repositoryId,
            new Map<string, Set<string>>([
                [ca, new Set()],
                [cb, new Set([ca])],
            ])
        )

        // Add dumps
        await util.insertDump(connection, dumpManager, repositoryId, cb, 'root1/')
        await util.insertDump(connection, dumpManager, repositoryId, cb, 'root2/')

        // Test closest commit
        expect(await dumpManager.findClosestDump(repositoryId, ca, 'blah')).toBeUndefined()
        expect(pick(await dumpManager.findClosestDump(repositoryId, cb, 'root1/file.ts'), ...fields)).toEqual({
            repositoryId,
            commit: cb,
            root: 'root1/',
        })
        expect(pick(await dumpManager.findClosestDump(repositoryId, cb, 'root2/file.ts'), ...fields)).toEqual({
            repositoryId,
            commit: cb,
            root: 'root2/',
        })
        expect(pick(await dumpManager.findClosestDump(repositoryId, ca, 'root2/file.ts'), ...fields)).toEqual({
            repositoryId,
            commit: cb,
            root: 'root2/',
        })

        expect(await dumpManager.findClosestDump(repositoryId, ca, 'root3/file.ts')).toBeUndefined()

        await util.insertDump(connection, dumpManager, repositoryId, cb, '')
        expect(pick(await dumpManager.findClosestDump(repositoryId, ca, 'root2/file.ts'), ...fields)).toEqual({
            repositoryId,
            commit: cb,
            root: '',
        })
        expect(pick(await dumpManager.findClosestDump(repositoryId, ca, 'root3/file.ts'), ...fields)).toEqual({
            repositoryId,
            commit: cb,
            root: '',
        })
    })

    it('should not return elements farther than MAX_TRAVERSAL_LIMIT', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This repository has the following commit graph (ancestors to the left):
        //
        // MAX_TRAVERSAL_LIMIT -- ... -- 2 -- 1 -- 0
        //
        // Note: we use 'a' as a suffix for commit numbers on construction so that
        // we can distinguish `1` and `11` (`1a1a1a...` and `11a11a11a..`).

        const repositoryId = nextId()
        const c0 = util.createCommit(0)
        const c1 = util.createCommit(1)
        const cpen = util.createCommit(MAX_TRAVERSAL_LIMIT / 2 - 1)
        const cmax = util.createCommit(MAX_TRAVERSAL_LIMIT / 2)

        const commits = new Map<string, Set<string>>(
            Array.from({ length: MAX_TRAVERSAL_LIMIT }, (_, i) => [
                util.createCommit(i),
                new Set([util.createCommit(i + 1)]),
            ])
        )

        // Add relations
        await dumpManager.updateCommits(repositoryId, commits)

        // Add dumps
        await util.insertDump(connection, dumpManager, repositoryId, c0, '')

        const d1 = await dumpManager.findClosestDump(repositoryId, c0, 'file')
        const d2 = await dumpManager.findClosestDump(repositoryId, c1, 'file')
        const d3 = await dumpManager.findClosestDump(repositoryId, cpen, 'file')

        // Test closest commit
        expect(d1?.commit).toEqual(c0)
        expect(d2?.commit).toEqual(c0)
        expect(d3?.commit).toEqual(c0)

        // (Assuming MAX_TRAVERSAL_LIMIT = 100)
        // At commit `50`, the traversal limit will be reached before visiting commit `0`
        // because commits are visited in this order:
        //
        // | depth | commit |
        // | ----- | ------ |
        // | 1     | 50     | (with direction 'A')
        // | 2     | 50     | (with direction 'D')
        // | 3     | 51     |
        // | 4     | 49     |
        // | 5     | 52     |
        // | 6     | 48     |
        // | ...   |        |
        // | 99    | 99     |
        // | 100   | 1      | (limit reached)

        expect(await dumpManager.findClosestDump(repositoryId, cmax, 'file')).toBeUndefined()

        // Add closer dump
        await util.insertDump(connection, dumpManager, repositoryId, c1, '')

        // Now commit 1 should be found
        const dump = await dumpManager.findClosestDump(repositoryId, cmax, 'file')
        expect(dump?.commit).toEqual(c1)
    })

    it('should prune overlapping roots during visibility check', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This database has the following commit graph:
        //
        // a -- b -- c -- d -- e -- f -- g

        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()
        const cd = util.createCommit()
        const ce = util.createCommit()
        const cf = util.createCommit()
        const cg = util.createCommit()

        // Add relations
        await dumpManager.updateCommits(
            repositoryId,
            new Map<string, Set<string>>([
                [ca, new Set()],
                [cb, new Set([ca])],
                [cc, new Set([cb])],
                [cd, new Set([cc])],
                [ce, new Set([cd])],
                [cf, new Set([ce])],
                [cg, new Set([cf])],
            ])
        )

        // Add dumps
        await util.insertDump(connection, dumpManager, repositoryId, ca, 'r1')
        await util.insertDump(connection, dumpManager, repositoryId, cb, 'r2')
        await util.insertDump(connection, dumpManager, repositoryId, cc, '') // overwrites r1, r2
        const d1 = await util.insertDump(connection, dumpManager, repositoryId, cd, 'r3') // overwrites ''
        const d2 = await util.insertDump(connection, dumpManager, repositoryId, cf, 'r4')
        await util.insertDump(connection, dumpManager, repositoryId, cg, 'r5') // not traversed

        await dumpManager.updateDumpsVisibleFromTip(repositoryId, cf)
        const visibleDumps = await dumpManager.getVisibleDumps(repositoryId)
        expect(visibleDumps.map((dump: pgModels.LsifDump) => dump.id).sort()).toEqual([d1.id, d2.id])
    })

    it('should traverse branching paths during visibility check', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This database has the following commit graph:
        //
        // a --+-- [b] --- c ---+
        //     |                |
        //     +--- d --- [e] --+ -- [h] --+-- [i]
        //     |                           |
        //     +-- [f] --- g --------------+

        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()
        const cd = util.createCommit()
        const ce = util.createCommit()
        const ch = util.createCommit()
        const ci = util.createCommit()
        const cf = util.createCommit()
        const cg = util.createCommit()

        // Add relations
        await dumpManager.updateCommits(
            repositoryId,
            new Map<string, Set<string>>([
                [ca, new Set()],
                [cb, new Set([ca])],
                [cc, new Set([cb])],
                [cd, new Set([ca])],
                [ce, new Set([cd])],
                [ch, new Set([cc, ce])],
                [ci, new Set([ch, cg])],
                [cf, new Set([ca])],
                [cg, new Set([cf])],
            ])
        )

        // Add dumps
        await util.insertDump(connection, dumpManager, repositoryId, cb, 'r2')
        const dump1 = await util.insertDump(connection, dumpManager, repositoryId, ce, 'r2/a') // overwrites r2 in commit b
        const dump2 = await util.insertDump(connection, dumpManager, repositoryId, ce, 'r2/b')
        await util.insertDump(connection, dumpManager, repositoryId, cf, 'r1/a')
        await util.insertDump(connection, dumpManager, repositoryId, cf, 'r1/b')
        const dump3 = await util.insertDump(connection, dumpManager, repositoryId, ch, 'r1') // overwrites r1/{a,b} in commit f
        const dump4 = await util.insertDump(connection, dumpManager, repositoryId, ci, 'r3')

        await dumpManager.updateDumpsVisibleFromTip(repositoryId, ci)
        const visibleDumps = await dumpManager.getVisibleDumps(repositoryId)
        expect(visibleDumps.map((dump: pgModels.LsifDump) => dump.id).sort()).toEqual([
            dump1.id,
            dump2.id,
            dump3.id,
            dump4.id,
        ])
    })

    it('should not set dumps visible farther than MAX_TRAVERSAL_LIMIT', async () => {
        if (!dumpManager) {
            fail('failed beforeAll')
        }

        // This repository has the following commit graph (ancestors to the left):
        //
        // (MAX_TRAVERSAL_LIMIT + 1) -- ... -- 2 -- 1 -- 0
        //
        // Note: we use 'a' as a suffix for commit numbers on construction so that
        // we can distinguish `1` and `11` (`1a1a1a...` and `11a11a11a...`).

        const repositoryId = nextId()
        const c0 = util.createCommit(0)
        const c1 = util.createCommit(1)
        const cpen = util.createCommit(MAX_TRAVERSAL_LIMIT - 1)
        const cmax = util.createCommit(MAX_TRAVERSAL_LIMIT)

        const commits = new Map<string, Set<string>>(
            Array.from({ length: MAX_TRAVERSAL_LIMIT + 1 }, (_, i) => [
                util.createCommit(i),
                new Set([util.createCommit(i + 1)]),
            ])
        )

        // Add relations
        await dumpManager.updateCommits(repositoryId, commits)

        // Add dumps
        const dump1 = await util.insertDump(connection, dumpManager, repositoryId, cmax, '')

        await dumpManager.updateDumpsVisibleFromTip(repositoryId, cmax)
        let visibleDumps = await dumpManager.getVisibleDumps(repositoryId)
        expect(visibleDumps.map((dump: pgModels.LsifDump) => dump.id).sort()).toEqual([dump1.id])

        await dumpManager.updateDumpsVisibleFromTip(repositoryId, c1)
        visibleDumps = await dumpManager.getVisibleDumps(repositoryId)
        expect(visibleDumps.map((dump: pgModels.LsifDump) => dump.id).sort()).toEqual([dump1.id])

        await dumpManager.updateDumpsVisibleFromTip(repositoryId, c0)
        visibleDumps = await dumpManager.getVisibleDumps(repositoryId)
        expect(visibleDumps.map((dump: pgModels.LsifDump) => dump.id).sort()).toEqual([])

        // Add closer dump
        const dump2 = await util.insertDump(connection, dumpManager, repositoryId, cpen, '')

        // Now commit cpen should be found
        await dumpManager.updateDumpsVisibleFromTip(repositoryId, c0)
        visibleDumps = await dumpManager.getVisibleDumps(repositoryId)
        expect(visibleDumps.map((dump: pgModels.LsifDump) => dump.id).sort()).toEqual([dump2.id])
    })
})

describe('discoverAndUpdateCommit', () => {
    let counter = 200
    const nextId = () => {
        counter++
        return counter
    }

    it('should update tracked commits', async () => {
        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()

        nock('http://frontend')
            .post(`/.internal/git/${repositoryId}/exec`)
            .reply(200, `${ca}\n${cb} ${ca}\n${cc} ${cb}`)

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const dumpManager = new DumpManager(connection, '')
            await util.insertDump(connection, dumpManager, repositoryId, ca, '')

            await dumpManager.updateCommits(
                repositoryId,
                await dumpManager.discoverCommits({
                    repositoryId,
                    commit: cc,
                    frontendUrl: 'frontend',
                })
            )

            // Ensure all commits are now tracked
            expect((await connection.getRepository(pgModels.Commit).find()).map(c => c.commit).sort()).toEqual([
                ca,
                cb,
                cc,
            ])
        } finally {
            await cleanup()
        }
    })

    it('should early-out if commit is tracked', async () => {
        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const dumpManager = new DumpManager(connection, '')
            await util.insertDump(connection, dumpManager, repositoryId, ca, '')
            await dumpManager.updateCommits(
                repositoryId,
                new Map<string, Set<string>>([[cb, new Set()]])
            )

            // This test ensures the following does not make a gitserver request.
            // As we did not register a nock interceptor, any request will result
            // in an exception being thrown.

            await dumpManager.updateCommits(
                repositoryId,
                await dumpManager.discoverCommits({
                    repositoryId,
                    commit: cb,
                    frontendUrl: 'frontend',
                })
            )
        } finally {
            await cleanup()
        }
    })

    it('should early-out if repository is unknown', async () => {
        const repositoryId = nextId()
        const ca = util.createCommit()

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const dumpManager = new DumpManager(connection, '')

            // This test ensures the following does not make a gitserver request.
            // As we did not register a nock interceptor, any request will result
            // in an exception being thrown.

            await dumpManager.updateCommits(
                repositoryId,
                await dumpManager.discoverCommits({
                    repositoryId,
                    commit: ca,
                    frontendUrl: 'frontend',
                })
            )
        } finally {
            await cleanup()
        }
    })
})

describe('discoverAndUpdateTips', () => {
    let counter = 300
    const nextId = () => {
        counter++
        return counter
    }

    it('should update tips', async () => {
        const repositoryId = nextId()
        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()
        const cd = util.createCommit()
        const ce = util.createCommit()

        nock('http://frontend')
            .post(`/.internal/git/${repositoryId}/exec`, { args: ['rev-parse', 'HEAD'] })
            .reply(200, ce)

        const { connection, cleanup } = await util.createCleanPostgresDatabase()

        try {
            const dumpManager = new DumpManager(connection, '')
            await dumpManager.updateCommits(
                repositoryId,
                new Map<string, Set<string>>([
                    [ca, new Set<string>()],
                    [cb, new Set<string>([ca])],
                    [cc, new Set<string>([cb])],
                    [cd, new Set<string>([cc])],
                    [ce, new Set<string>([cd])],
                ])
            )
            await util.insertDump(connection, dumpManager, repositoryId, ca, 'foo')
            await util.insertDump(connection, dumpManager, repositoryId, cb, 'foo')
            await util.insertDump(connection, dumpManager, repositoryId, cc, 'bar')

            const tipCommit = await dumpManager.discoverTip({
                repositoryId,
                frontendUrl: 'frontend',
            })
            if (!tipCommit) {
                throw new Error('Expected a tip commit')
            }
            await dumpManager.updateDumpsVisibleFromTip(repositoryId, tipCommit)

            const d1 = await dumpManager.getDump(repositoryId, ca, 'foo/test.ts')
            const d2 = await dumpManager.getDump(repositoryId, cb, 'foo/test.ts')
            const d3 = await dumpManager.getDump(repositoryId, cc, 'bar/test.ts')

            expect(d1?.visibleAtTip).toBeFalsy()
            expect(d2?.visibleAtTip).toBeTruthy()
            expect(d3?.visibleAtTip).toBeTruthy()
        } finally {
            await cleanup()
        }
    })
})
