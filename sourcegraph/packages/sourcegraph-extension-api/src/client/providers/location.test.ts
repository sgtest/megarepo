import * as assert from 'assert'
import { of } from 'rxjs'
import { TestScheduler } from 'rxjs/testing'
import { Location } from '../../protocol/plainTypes'
import { getLocation, getLocations, ProvideTextDocumentLocationSignature } from './location'
import { FIXTURE } from './registry.test'

const scheduler = () => new TestScheduler((a, b) => assert.deepStrictEqual(a, b))

const FIXTURE_LOCATION: Location = {
    uri: 'file:///f',
    range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
}
const FIXTURE_LOCATIONS: Location | Location[] | null = [FIXTURE_LOCATION, FIXTURE_LOCATION]

describe('getLocation', () => {
    describe('0 providers', () => {
        it('returns null', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', { a: [] }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: null,
                })
            ))
    })

    describe('1 provider', () => {
        it('returns null result from provider', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', { a: [() => of(null)] }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: null,
                })
            ))

        it('returns result array from provider', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                            a: [() => of(FIXTURE_LOCATIONS)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: FIXTURE_LOCATIONS,
                })
            ))

        it('returns single result from provider', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                            a: [() => of(FIXTURE_LOCATION)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: FIXTURE_LOCATION,
                })
            ))
    })

    describe('2 providers', () => {
        it('returns null result if both providers return null', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                            a: [() => of(null), () => of(null)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: null,
                })
            ))

        it('omits null result from 1 provider', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                            a: [() => of(FIXTURE_LOCATIONS), () => of(null)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: FIXTURE_LOCATIONS,
                })
            ))

        it('merges results from providers', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                            a: [
                                () =>
                                    of({
                                        uri: 'file:///f1',
                                        range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
                                    }),
                                () =>
                                    of({
                                        uri: 'file:///f2',
                                        range: { start: { line: 5, character: 6 }, end: { line: 7, character: 8 } },
                                    }),
                            ],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: [
                        {
                            uri: 'file:///f1',
                            range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
                        },
                        {
                            uri: 'file:///f2',
                            range: { start: { line: 5, character: 6 }, end: { line: 7, character: 8 } },
                        },
                    ],
                })
            ))
    })

    describe('multiple emissions', () => {
        it('returns stream of results', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getLocation(
                        cold<ProvideTextDocumentLocationSignature[]>('-a-b-|', {
                            a: [() => of(FIXTURE_LOCATIONS)],
                            b: [() => of(null)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-b-|', {
                    a: FIXTURE_LOCATIONS,
                    b: null,
                })
            ))
    })
})

describe('getLocations', () => {
    it('wraps single result in array', () =>
        scheduler().run(({ cold, expectObservable }) =>
            expectObservable(
                getLocations(
                    cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                        a: [() => of(FIXTURE_LOCATION)],
                    }),
                    FIXTURE.TextDocumentPositionParams
                )
            ).toBe('-a-|', {
                a: [FIXTURE_LOCATION],
            })
        ))

    it('preserves array results', () =>
        scheduler().run(({ cold, expectObservable }) =>
            expectObservable(
                getLocations(
                    cold<ProvideTextDocumentLocationSignature[]>('-a-|', {
                        a: [() => of(FIXTURE_LOCATIONS)],
                    }),
                    FIXTURE.TextDocumentPositionParams
                )
            ).toBe('-a-|', {
                a: FIXTURE_LOCATIONS,
            })
        ))
})
