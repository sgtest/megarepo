import { ProxyMarked, proxyMarker, Remote } from 'comlink'
import { Observer, of } from 'rxjs'

import { SettingsCascade } from '../../../settings/settings'
import { MainThreadAPI } from '../../contract'
import { pretendProxySubscribable, pretendRemote } from '../../util'
import { FileDecorationsByPath } from '../extensionHostApi'

import { initializeExtensionHostTest } from './test-helpers'

describe('extensionHostAPI.getFileDecorations()', () => {
    // integration(ish) tests for scenarios not covered by providers tests
    const noopMain = pretendRemote<MainThreadAPI>({
        getEnabledExtensions: () => pretendProxySubscribable(of([])),
        getScriptURLForExtension: () => undefined,
    })
    const emptySettings: SettingsCascade<object> = {
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

    it('restarts hover call if a provider was added or removed', () => {
        const { extensionAPI, extensionHostAPI } = initializeExtensionHostTest(
            {
                initialSettings: emptySettings,
                clientApplication: 'sourcegraph',
            },
            noopMain
        )

        let counter = 0
        extensionAPI.app.registerFileDecorationProvider({
            provideFileDecorations: ({ files }) => [{ uri: files[0]?.uri, after: { contentText: `a${++counter}` } }],
        })

        let results: FileDecorationsByPath[] = []
        extensionHostAPI
            .getFileDecorations({
                uri: 'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/',
                files: [
                    {
                        uri:
                            'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/package.ts',
                        isDirectory: false,
                        path: 'extensions/typescript/src/package.ts',
                    },
                ],
            })
            .subscribe(observe(value => results.push(value)))

        // first provider results
        expect(results).toEqual<FileDecorationsByPath[]>([
            {},
            {
                'extensions/typescript/src/package.ts': [
                    {
                        uri:
                            'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/package.ts',
                        after: {
                            contentText: 'a1',
                        },
                    },
                ],
            },
        ])
        results = []

        const subscription = extensionAPI.app.registerFileDecorationProvider({
            provideFileDecorations: ({ files }) => [{ uri: files[0]?.uri, after: { contentText: 'b' } }],
        })

        // second and first
        expect(results).toEqual<FileDecorationsByPath[]>([
            {
                'extensions/typescript/src/package.ts': [
                    {
                        uri:
                            'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/package.ts',
                        after: {
                            contentText: 'a2',
                        },
                    },
                ],
            },
            {
                'extensions/typescript/src/package.ts': [
                    {
                        uri:
                            'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/package.ts',
                        after: {
                            contentText: 'a2',
                        },
                    },
                    {
                        uri:
                            'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/package.ts',
                        after: {
                            contentText: 'b',
                        },
                    },
                ],
            },
        ])
        results = []

        subscription.unsubscribe()

        // just first was queried for the third time
        expect(results).toEqual<FileDecorationsByPath[]>([
            {},
            {
                'extensions/typescript/src/package.ts': [
                    {
                        uri:
                            'git://gitlab.sgdev.org/sourcegraph/code-intel-extensions?1111#extensions/typescript/src/package.ts',
                        after: {
                            contentText: 'a3',
                        },
                    },
                ],
            },
        ])
        results = []
    })
})
