import { ProxyMarked, proxyMarker, Remote } from 'comlink'
import { BehaviorSubject, Observer } from 'rxjs'
import { DocumentHighlight } from 'sourcegraph'

import { Range } from '@sourcegraph/extension-api-classes'

import { SettingsCascade } from '../../../settings/settings'
import { MainThreadAPI } from '../../contract'
import { pretendRemote } from '../../util'
import { proxySubscribable } from '../api/common'

import { initializeExtensionHostTest } from './test-helpers'

describe('getDocumentHighlights from ExtensionHost API, it aims to have more e2e feel', () => {
    // integration(ish) tests for scenarios not covered by providers tests
    const noopMain = pretendRemote<MainThreadAPI>({
        getEnabledExtensions: () => proxySubscribable(new BehaviorSubject([])),
        getScriptURLForExtension: () => undefined,
    })
    const initialSettings: SettingsCascade<object> = {
        subjects: [],
        final: {},
    }

    const observe = <T>(onValue: (value: T) => void): Remote<Observer<T> & ProxyMarked> =>
        pretendRemote({
            next: onValue,
            error: (error: any) => {
                throw error
            },
            complete: () => {},
            [proxyMarker]: Promise.resolve(true as const),
        })

    const documentHighlight = (value: number): DocumentHighlight => ({
        range: new Range(value, value, value, value),
    })

    it('restarts document highlights call if a provider was added or removed', () => {
        const typescriptFileUri = 'file:///f.ts'

        const { extensionHostAPI, extensionAPI } = initializeExtensionHostTest(
            { initialSettings, clientApplication: 'sourcegraph' },
            noopMain
        )

        extensionHostAPI.addTextDocumentIfNotExists({
            languageId: 'ts',
            text: 'body',
            uri: typescriptFileUri,
        })

        let counter = 0
        extensionAPI.languages.registerDocumentHighlightProvider([{ pattern: '*.ts' }], {
            provideDocumentHighlights: () => [documentHighlight(++counter)],
        })

        let results: DocumentHighlight[][] = []
        extensionHostAPI
            .getDocumentHighlights({
                position: { line: 1, character: 2 },
                textDocument: { uri: typescriptFileUri },
            })
            .subscribe(observe(value => results.push(value)))

        // first provider results
        expect(results).toEqual([[], [documentHighlight(1)]])
        results = []

        const subscription = extensionAPI.languages.registerDocumentHighlightProvider([{ pattern: '*.ts' }], {
            provideDocumentHighlights: () => [documentHighlight(0)],
        })

        // second and first
        expect(results).toEqual([[], [2, 0].map(value => documentHighlight(value))])
        results = []

        subscription.unsubscribe()

        // just first was queried for the third time
        expect(results).toEqual([[], [documentHighlight(3)]])
    })
})
