import * as lsp from 'vscode-languageserver-protocol'
import { comparePosition, createRemoteUri, mapRangesToLocations, findRanges } from './database'
import { MonikerId, RangeData, RangeId } from './database.models'

describe('findRanges', () => {
    it('should return ranges containing position', () => {
        const range1 = {
            startLine: 0,
            startCharacter: 3,
            endLine: 0,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range2 = {
            startLine: 1,
            startCharacter: 3,
            endLine: 1,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range3 = {
            startLine: 2,
            startCharacter: 3,
            endLine: 2,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range4 = {
            startLine: 3,
            startCharacter: 3,
            endLine: 3,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range5 = {
            startLine: 4,
            startCharacter: 3,
            endLine: 4,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }

        expect(findRanges([range1, range2, range3, range4, range5], { line: 0, character: 4 })).toEqual([range1])
        expect(findRanges([range1, range2, range3, range4, range5], { line: 1, character: 4 })).toEqual([range2])
        expect(findRanges([range1, range2, range3, range4, range5], { line: 2, character: 4 })).toEqual([range3])
        expect(findRanges([range1, range2, range3, range4, range5], { line: 3, character: 4 })).toEqual([range4])
        expect(findRanges([range1, range2, range3, range4, range5], { line: 4, character: 4 })).toEqual([range5])
    })

    it('should order inner-most ranges first', () => {
        const range1 = {
            startLine: 0,
            startCharacter: 3,
            endLine: 4,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range2 = {
            startLine: 1,
            startCharacter: 3,
            endLine: 3,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range3 = {
            startLine: 2,
            startCharacter: 3,
            endLine: 2,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range4 = {
            startLine: 5,
            startCharacter: 3,
            endLine: 5,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }
        const range5 = {
            startLine: 6,
            startCharacter: 3,
            endLine: 6,
            endCharacter: 5,
            monikerIds: new Set<MonikerId>(),
        }

        expect(findRanges([range1, range2, range3, range4, range5], { line: 2, character: 4 })).toEqual([
            range3,
            range2,
            range1,
        ])
    })
})

describe('comparePosition', () => {
    it('should return the relative order to a range', () => {
        const range = {
            startLine: 5,
            startCharacter: 11,
            endLine: 5,
            endCharacter: 13,
            monikerIds: new Set<MonikerId>(),
        }

        expect(comparePosition(range, { line: 5, character: 11 })).toEqual(0)
        expect(comparePosition(range, { line: 5, character: 12 })).toEqual(0)
        expect(comparePosition(range, { line: 5, character: 13 })).toEqual(0)
        expect(comparePosition(range, { line: 4, character: 12 })).toEqual(+1)
        expect(comparePosition(range, { line: 5, character: 10 })).toEqual(+1)
        expect(comparePosition(range, { line: 5, character: 14 })).toEqual(-1)
        expect(comparePosition(range, { line: 6, character: 12 })).toEqual(-1)
    })
})

describe('createRemoteUri', () => {
    it('should generate a URI to another project', () => {
        const pkg = {
            id: 0,
            scheme: '',
            name: '',
            version: '',
            dump: {
                id: 0,
                repository: 'github.com/sourcegraph/codeintellify',
                commit: 'deadbeef',
                root: '',
                visible_at_tip: false,
            },
            dump_id: 0,
        }

        const uri = createRemoteUri(pkg, 'src/position.ts')
        expect(uri).toEqual('git://github.com/sourcegraph/codeintellify?deadbeef#src/position.ts')
    })
})

describe('mapRangesToLocations', () => {
    it('should map ranges to locations', () => {
        const ranges = new Map<RangeId, RangeData>()
        ranges.set(1, {
            startLine: 1,
            startCharacter: 1,
            endLine: 1,
            endCharacter: 2,
            monikerIds: new Set<MonikerId>(),
        })
        ranges.set(2, {
            startLine: 3,
            startCharacter: 1,
            endLine: 3,
            endCharacter: 2,
            monikerIds: new Set<MonikerId>(),
        })
        ranges.set(4, {
            startLine: 2,
            startCharacter: 1,
            endLine: 2,
            endCharacter: 2,
            monikerIds: new Set<MonikerId>(),
        })

        const locations = mapRangesToLocations(ranges, 'src/position.ts', new Set([1, 2, 4]))
        expect(locations).toContainEqual(
            lsp.Location.create('src/position.ts', { start: { line: 1, character: 1 }, end: { line: 1, character: 2 } })
        )
        expect(locations).toContainEqual(
            lsp.Location.create('src/position.ts', { start: { line: 3, character: 1 }, end: { line: 3, character: 2 } })
        )
        expect(locations).toContainEqual(
            lsp.Location.create('src/position.ts', { start: { line: 2, character: 1 }, end: { line: 2, character: 2 } })
        )
        expect(locations).toHaveLength(3)
    })
})
