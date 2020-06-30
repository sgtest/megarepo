import { MarkupKind } from '@sourcegraph/extension-api-classes'
import { Hover, Range } from 'sourcegraph'
import { HoverMerged } from '../client/types/hover'
import { initNewExtensionAPI, mergeHoverResults } from './flatExtensionApi'
import { pretendRemote } from '../util'
import { MainThreadAPI } from '../contract'
import { SettingsCascade } from '../../settings/settings'
import { Observer } from 'rxjs'
import { ProxyMarked, proxyMarker, Remote } from 'comlink'
import { ExtensionDocuments } from './api/documents'
import { MaybeLoadingResult, LOADING } from '@sourcegraph/codeintellify'

describe('mergeHoverResults', () => {
    it('merges non Hover values into nulls', () => {
        expect(mergeHoverResults([LOADING])).toBe(null)
        expect(mergeHoverResults([null])).toBe(null)
        expect(mergeHoverResults([undefined])).toBe(null)
        // and yes, there can be several
        expect(mergeHoverResults([null, LOADING])).toBe(null)
    })

    it('merges a Hover into result', () => {
        const hover: Hover = { contents: { value: 'a', kind: MarkupKind.PlainText } }
        const merged: HoverMerged = { contents: [hover.contents] }
        expect(mergeHoverResults([hover])).toEqual(merged)
    })

    it('omits non Hover values from hovers result', () => {
        const hover: Hover = { contents: { value: 'a', kind: MarkupKind.PlainText } }
        const merged: HoverMerged = { contents: [hover.contents] }
        expect(mergeHoverResults([hover, null, LOADING, undefined])).toEqual(merged)
    })

    it('merges Hovers with ranges', () => {
        const hover1: Hover = {
            contents: { value: 'c1' },
            // TODO this is weird to cast to ranges
            range: ({ start: { line: 1, character: 2 }, end: { line: 3, character: 4 } } as unknown) as Range,
        }
        const hover2: Hover = {
            contents: { value: 'c2' },
            // TODO this is weird to cast to ranges
            range: ({ start: { line: 1, character: 2 }, end: { line: 3, character: 4 } } as unknown) as Range,
        }
        const merged: HoverMerged = {
            contents: [
                { value: 'c1', kind: MarkupKind.PlainText },
                { value: 'c2', kind: MarkupKind.PlainText },
            ],
            range: { start: { line: 1, character: 2 }, end: { line: 3, character: 4 } },
        }

        expect(mergeHoverResults([hover1, hover2])).toEqual(merged)
    })
})

describe('getHover from ExtensionHost API, it aims to have more e2e feel', () => {
    // integration(ish) tests for scenarios not covered by providers tests
    const noopMain = pretendRemote<MainThreadAPI>({})
    const emptySettings: SettingsCascade<object> = {
        subjects: [],
        final: {},
    }

    const observe = <T>(onValue: (val: T) => void): Remote<Observer<T> & ProxyMarked> =>
        pretendRemote({
            next: onValue,
            error: (error: any) => {
                throw error
            },
            complete: () => {},
            [proxyMarker]: Promise.resolve(true as const),
        })

    const textHover = (value: string): Hover => ({
        contents: { value, kind: MarkupKind.PlainText },
    })

    it('restarts hover call if a provider was added or removed', () => {
        const typescriptFileUri = 'file:///f.ts'
        const documents = new ExtensionDocuments(() => Promise.resolve())
        documents.$acceptDocumentData([
            {
                type: 'added',
                languageId: 'ts',
                text: 'body',
                uri: typescriptFileUri,
            },
        ])

        const { exposedToMain, languages } = initNewExtensionAPI(noopMain, emptySettings, documents)

        let counter = 0
        languages.registerHoverProvider([{ pattern: '*.ts' }], {
            provideHover: () => textHover(`a${++counter}`),
        })

        let results: any[] = []
        exposedToMain
            .getHover({
                position: { line: 1, character: 2 },
                textDocument: { uri: typescriptFileUri },
            })
            .subscribe(observe(value => results.push(value)))

        // first provider results
        expect(results).toEqual<MaybeLoadingResult<HoverMerged | null>[]>([
            { isLoading: true, result: null },
            {
                isLoading: false,
                result: { contents: [textHover('a1').contents] },
            },
        ])
        results = []

        const subscription = languages.registerHoverProvider([{ pattern: '*.ts' }], {
            provideHover: () => textHover('b'),
        })

        // second and first
        expect(results).toEqual<MaybeLoadingResult<HoverMerged | null>[]>([
            {
                isLoading: true,
                result: { contents: [textHover('a2').contents] },
            },
            {
                isLoading: false,
                result: {
                    contents: ['a2', 'b'].map(value => textHover(value).contents),
                },
            },
        ])
        results = []

        subscription.unsubscribe()

        // just first was queried for the third time
        expect(results).toEqual<MaybeLoadingResult<HoverMerged | null>[]>([
            { isLoading: true, result: null },
            {
                isLoading: false,
                result: { contents: [textHover('a3').contents] },
            },
        ])
    })
})
