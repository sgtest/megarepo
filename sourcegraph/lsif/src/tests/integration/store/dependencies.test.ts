import * as util from '../integration-test-util'
import * as pgModels from '../../../shared/models/pg'
import rmfr from 'rmfr'
import { Connection } from 'typeorm'
import { fail } from 'assert'
import { DumpManager } from '../../../shared/store/dumps'
import { DependencyManager } from '../../../shared/store/dependencies'

describe('DependencyManager', () => {
    let connection!: Connection
    let cleanup!: () => Promise<void>
    let storageRoot!: string
    let dumpManager!: DumpManager
    let dependencyManager!: DependencyManager

    beforeAll(async () => {
        ;({ connection, cleanup } = await util.createCleanPostgresDatabase())
        storageRoot = await util.createStorageRoot()
        dumpManager = new DumpManager(connection, storageRoot)
        dependencyManager = new DependencyManager(connection)
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

    it('should respect bloom filter', async () => {
        if (!dependencyManager) {
            fail('failed beforeAll')
        }

        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()
        const cd = util.createCommit()
        const ce = util.createCommit()
        const cf = util.createCommit()

        const updatePackages = async (
            commit: string,
            root: string,
            identifiers: string[]
        ): Promise<pgModels.LsifDump> => {
            const dump = await dumpManager.insertDump('foo', commit, root, new Date())

            await dependencyManager.addPackagesAndReferences(
                dump.id,
                [],
                [
                    {
                        package: {
                            scheme: 'npm',
                            name: 'p1',
                            version: '0.1.0',
                        },
                        identifiers,
                    },
                ]
            )

            return dump
        }

        // Note: roots must be unique so dumps are visible
        const dumpa = await updatePackages(ca, 'r1', ['x', 'y', 'z'])
        const dumpb = await updatePackages(cb, 'r2', ['y', 'z'])
        const dumpf = await updatePackages(cf, 'r3', ['y', 'z'])
        await updatePackages(cc, 'r4', ['x', 'z'])
        await updatePackages(cd, 'r5', ['x'])
        await updatePackages(ce, 'r6', ['x', 'z'])

        const getReferencedDumpIds = async () => {
            const { references } = await dependencyManager.getReferences({
                repository: '',
                scheme: 'npm',
                name: 'p1',
                version: '0.1.0',
                identifier: 'y',
                limit: 50,
                offset: 0,
            })

            return references.map(reference => reference.dump_id).sort()
        }

        await dumpManager.updateCommits(
            'foo',
            new Map<string, Set<string>>([
                [ca, new Set()],
                [cb, new Set([ca])],
                [cc, new Set([cb])],
                [cd, new Set([cc])],
                [ce, new Set([cd])],
                [cf, new Set([ce])],
            ])
        )
        await dumpManager.updateDumpsVisibleFromTip('foo', cf)

        // only references containing identifier y
        expect(await getReferencedDumpIds()).toEqual([dumpa.id, dumpb.id, dumpf.id])
    })

    it('should re-query if bloom filter prunes too many results', async () => {
        if (!dependencyManager) {
            fail('failed beforeAll')
        }

        const updatePackages = async (
            commit: string,
            root: string,
            identifiers: string[]
        ): Promise<pgModels.LsifDump> => {
            const dump = await dumpManager.insertDump('foo', commit, root, new Date())

            await dependencyManager.addPackagesAndReferences(
                dump.id,
                [],
                [
                    {
                        package: {
                            scheme: 'npm',
                            name: 'p1',
                            version: '0.1.0',
                        },
                        identifiers,
                    },
                ]
            )

            return dump
        }

        const dumps = []
        for (let i = 0; i < 250; i++) {
            // Spread out uses of `y` so that we pull back a series of pages that are
            // empty and half-empty after being filtered by the bloom filter. We will
            // have to empty pages (i < 100) followed by three pages where very third
            // uses the identifier. In all, there are fifty uses spread over 5 pages.
            const isUse = i >= 100 && i % 3 === 0

            const dump = await updatePackages(util.createCommit(), `r${i}`, ['x', isUse ? 'y' : 'z'])
            dump.visibleAtTip = true
            await connection.getRepository(pgModels.LsifDump).save(dump)

            if (isUse) {
                // Save use ids
                dumps.push(dump.id)
            }
        }

        const { references } = await dependencyManager.getReferences({
            repository: 'bar',
            scheme: 'npm',
            name: 'p1',
            version: '0.1.0',
            identifier: 'y',
            limit: 50,
            offset: 0,
        })

        expect(references.map(reference => reference.dump_id).sort()).toEqual(dumps)
    })

    it('references only returned if dumps visible at tip', async () => {
        if (!dependencyManager) {
            fail('failed beforeAll')
        }

        const ca = util.createCommit()
        const cb = util.createCommit()
        const cc = util.createCommit()

        const references = [
            {
                package: {
                    scheme: 'npm',
                    name: 'p1',
                    version: '0.1.0',
                },
                identifiers: ['x', 'y', 'z'],
            },
        ]

        const dumpa = await dumpManager.insertDump('foo', ca, '', new Date())
        const dumpb = await dumpManager.insertDump('foo', cb, '', new Date())
        const dumpc = await dumpManager.insertDump('foo', cc, '', new Date())

        await dependencyManager.addPackagesAndReferences(dumpa.id, [], references)
        await dependencyManager.addPackagesAndReferences(dumpb.id, [], references)
        await dependencyManager.addPackagesAndReferences(dumpc.id, [], references)

        const getReferencedDumpIds = async () =>
            (
                await dependencyManager.getReferences({
                    repository: '',
                    scheme: 'npm',
                    name: 'p1',
                    version: '0.1.0',
                    identifier: 'y',
                    limit: 50,
                    offset: 0,
                })
            ).references
                .map(reference => reference.dump_id)
                .sort()

        const updateVisibility = async (visibleA: boolean, visibleB: boolean, visibleC: boolean) => {
            dumpa.visibleAtTip = visibleA
            dumpb.visibleAtTip = visibleB
            dumpc.visibleAtTip = visibleC
            await connection.getRepository(pgModels.LsifDump).save(dumpa)
            await connection.getRepository(pgModels.LsifDump).save(dumpb)
            await connection.getRepository(pgModels.LsifDump).save(dumpc)
        }

        // Set a, b visible from tip
        await updateVisibility(true, true, false)
        expect(await getReferencedDumpIds()).toEqual([dumpa.id, dumpb.id])

        // Clear a, b visible from tip, set c visible fro
        await updateVisibility(false, false, true)
        expect(await getReferencedDumpIds()).toEqual([dumpc.id])

        // Clear all visible from tip
        await updateVisibility(false, false, false)
        expect(await getReferencedDumpIds()).toEqual([])
    })
})
