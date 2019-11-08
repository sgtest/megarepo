import * as util from '../integration-test-util'

describe('Backend', () => {
    const ctx = new util.BackendTestContext()

    beforeAll(async () => {
        await ctx.init()
        await Promise.all(
            ['a', 'b1', 'b2', 'b3', 'c1', 'c2', 'c3'].map(r =>
                ctx.convertTestData(r, util.createCommit(r), '', `xrepo/data/${r}.lsif.gz`)
            )
        )
    })

    afterAll(async () => {
        await ctx.teardown()
    })

    it('should find all cross-repo defs of `add` from repo a', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const definitions = await ctx.backend.definitions('a', util.createCommit('a'), 'src/index.ts', {
            line: 11,
            character: 18,
        })
        expect(definitions).toEqual([util.createLocation('src/index.ts', 0, 16, 0, 19)])
    })

    it('should find all cross-repo defs of `add` from repo b1', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const definitions = await ctx.backend.definitions('b1', util.createCommit('b1'), 'src/index.ts', {
            line: 3,
            character: 12,
        })
        expect(definitions).toEqual([util.createRemoteLocation('a', 'src/index.ts', 0, 16, 0, 19)])
    })

    it('should find all cross-repo defs of `mul` from repo b1', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const definitions = await ctx.backend.definitions('b1', util.createCommit('b1'), 'src/index.ts', {
            line: 3,
            character: 16,
        })
        expect(definitions).toEqual([util.createRemoteLocation('a', 'src/index.ts', 4, 16, 4, 19)])
    })

    it('should find all cross-repo refs of `mul` from repo a', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const { locations } = util.filterNodeModules(
            await ctx.backend.references('a', util.createCommit('a'), 'src/index.ts', {
                line: 4,
                character: 19,
            })
        )

        expect(locations).toContainEqual(util.createLocation('src/index.ts', 4, 16, 4, 19)) // def
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 0, 14, 0, 17)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 3, 15, 3, 18)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 3, 26, 3, 29)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 0, 14, 0, 17)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 3, 15, 3, 18)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 3, 26, 3, 29)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 0, 14, 0, 17)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 3, 15, 3, 18)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 3, 26, 3, 29)) // 2nd use

        // Ensure no additional references
        expect(locations && locations.length).toEqual(10)
    })

    it('should find all cross-repo refs of `mul` from repo b1', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const { locations } = util.filterNodeModules(
            await ctx.backend.references('b1', util.createCommit('b1'), 'src/index.ts', {
                line: 3,
                character: 16,
            })
        )

        expect(locations).toContainEqual(util.createRemoteLocation('a', 'src/index.ts', 4, 16, 4, 19)) // def
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 0, 14, 0, 17)) // import
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 3, 15, 3, 18)) // 1st use
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 3, 26, 3, 29)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 0, 14, 0, 17)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 3, 15, 3, 18)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 3, 26, 3, 29)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 0, 14, 0, 17)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 3, 15, 3, 18)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 3, 26, 3, 29)) // 2nd use

        // Ensure no additional references
        expect(locations && locations.length).toEqual(10)
    })

    it('should find all cross-repo refs of `add` from repo a', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const { locations } = util.filterNodeModules(
            await ctx.backend.references('a', util.createCommit('a'), 'src/index.ts', {
                line: 0,
                character: 17,
            })
        )

        expect(locations).toContainEqual(util.createLocation('src/index.ts', 0, 16, 0, 19)) // def
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 11, 18, 11, 21)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('c1', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('c1', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('c1', 'src/index.ts', 3, 15, 3, 18)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('c1', 'src/index.ts', 3, 26, 3, 29)) // 3rd use
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 3, 15, 3, 18)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 3, 26, 3, 29)) // 3rd use
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 3, 15, 3, 18)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 3, 26, 3, 29)) // 3rd use

        // Ensure no additional references
        expect(locations && locations.length).toEqual(20)
    })

    it('should find all cross-repo refs of `add` from repo c1', async () => {
        if (!ctx.backend) {
            fail('failed beforeAll')
            return
        }

        const { locations } = util.filterNodeModules(
            await ctx.backend.references('c1', util.createCommit('c1'), 'src/index.ts', {
                line: 3,
                character: 16,
            })
        )

        expect(locations).toContainEqual(util.createRemoteLocation('a', 'src/index.ts', 0, 16, 0, 19)) // def
        expect(locations).toContainEqual(util.createRemoteLocation('a', 'src/index.ts', 11, 18, 11, 21)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b1', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b2', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('b3', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 3, 15, 3, 18)) // 2nd use
        expect(locations).toContainEqual(util.createLocation('src/index.ts', 3, 26, 3, 29)) // 3rd use
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 3, 15, 3, 18)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('c2', 'src/index.ts', 3, 26, 3, 29)) // 3rd use
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 0, 9, 0, 12)) // import
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 3, 11, 3, 14)) // 1st use
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 3, 15, 3, 18)) // 2nd use
        expect(locations).toContainEqual(util.createRemoteLocation('c3', 'src/index.ts', 3, 26, 3, 29)) // 3rd use

        // Ensure no additional references
        expect(locations && locations.length).toEqual(20)
    })
})
