import * as assert from 'assert'
import { of, throwError } from 'rxjs'
import { TestScheduler } from 'rxjs/testing'
import { Hover, MarkupKind } from 'sourcegraph'
import { HoverMerged } from '../../client/types/hover'
import { getHover, ProvideTextDocumentHoverSignature } from './hover'
import { FIXTURE } from './registry.test'

const scheduler = () => new TestScheduler((a, b) => assert.deepStrictEqual(a, b))

const FIXTURE_RESULT: Hover | null = { contents: { value: 'c', kind: MarkupKind.PlainText } }
const FIXTURE_RESULT_MERGED: HoverMerged | null = { contents: [{ value: 'c', kind: MarkupKind.PlainText }] }

describe('getHover', () => {
    describe('0 providers', () => {
        it('returns null', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', { a: [] }),
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
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', { a: [() => of(null)] }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: null,
                })
            ))

        it('returns result from provider', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', {
                            a: [() => of(FIXTURE_RESULT)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: FIXTURE_RESULT_MERGED,
                })
            ))
    })

    describe('2 providers', () => {
        it('returns null result if both providers return null', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', {
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
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', {
                            a: [() => of(FIXTURE_RESULT), () => of(null)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: FIXTURE_RESULT_MERGED,
                })
            ))

        it('omits error result from 1 provider', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', {
                            a: [() => of(FIXTURE_RESULT), () => throwError('err')],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: FIXTURE_RESULT_MERGED,
                })
            ))

        it('merges results from providers', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-|', {
                            a: [
                                () =>
                                    of({
                                        contents: { value: 'c1' },
                                        range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
                                    }),
                                () =>
                                    of({
                                        contents: { value: 'c2' },
                                        range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
                                    }),
                            ],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-|', {
                    a: {
                        contents: [
                            { value: 'c1', kind: MarkupKind.PlainText },
                            { value: 'c2', kind: MarkupKind.PlainText },
                        ],
                        range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
                    },
                })
            ))
    })

    describe('multiple emissions', () => {
        it('returns stream of results', () =>
            scheduler().run(({ cold, expectObservable }) =>
                expectObservable(
                    getHover(
                        cold<ProvideTextDocumentHoverSignature[]>('-a-b-|', {
                            a: [() => of(FIXTURE_RESULT)],
                            b: [() => of(null)],
                        }),
                        FIXTURE.TextDocumentPositionParams
                    )
                ).toBe('-a-b-|', {
                    a: FIXTURE_RESULT_MERGED,
                    b: null,
                })
            ))
    })
})
