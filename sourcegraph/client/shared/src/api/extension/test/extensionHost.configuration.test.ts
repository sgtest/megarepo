import { proxy } from 'comlink'
import { BehaviorSubject } from 'rxjs'

import { SettingsCascade } from '../../../settings/settings'
import { SettingsEdit } from '../../client/services/settings'
import { MainThreadAPI } from '../../contract'
import { pretendRemote } from '../../util'
import { proxySubscribable } from '../api/common'

import { initializeExtensionHostTest } from './test-helpers'

const initialSettings = (value: { a: string }): SettingsCascade<{ a: string }> => ({
    subjects: [],
    final: value,
})

describe('ExtensionHost: Configuration', () => {
    describe('get()', () => {
        test('returns the latest settings', () => {
            const { extensionHostAPI, extensionAPI } = initializeExtensionHostTest(
                {
                    initialSettings: initialSettings({ a: 'a' }),
                    clientApplication: 'sourcegraph',
                },
                pretendRemote<MainThreadAPI>({
                    getScriptURLForExtension: proxy(() => undefined),
                    getEnabledExtensions: () => proxySubscribable(new BehaviorSubject([])),
                })
            )

            extensionHostAPI.syncSettingsData({ subjects: [], final: { a: 'b' } })
            extensionHostAPI.syncSettingsData({ subjects: [], final: { a: 'c' } })
            expect(extensionAPI.configuration.get<{ a: string }>().get('a')).toBe('c')
        })
    })

    describe('changes', () => {
        test('emits immediately on subscription', () => {
            const { extensionAPI } = initializeExtensionHostTest(
                {
                    initialSettings: initialSettings({ a: 'a' }),
                    clientApplication: 'sourcegraph',
                },
                pretendRemote<MainThreadAPI>({
                    getScriptURLForExtension: proxy(() => undefined),
                    getEnabledExtensions: () => proxySubscribable(new BehaviorSubject([])),
                })
            )

            let calledTimes = 0
            extensionAPI.configuration.subscribe(() => calledTimes++)
            expect(calledTimes).toBe(1)
        })

        test('emits when settings are updated', () => {
            const { extensionHostAPI, extensionAPI } = initializeExtensionHostTest(
                {
                    initialSettings: initialSettings({ a: 'a' }),
                    clientApplication: 'sourcegraph',
                },
                pretendRemote<MainThreadAPI>({
                    getScriptURLForExtension: proxy(() => undefined),
                    getEnabledExtensions: () => proxySubscribable(new BehaviorSubject([])),
                })
            )

            let calledTimes = 0
            extensionAPI.configuration.subscribe(() => calledTimes++)
            extensionHostAPI.syncSettingsData({ subjects: [], final: { a: 'b' } })
            // one initial and one update
            expect(calledTimes).toBe(2)
        })

        test('config objects freezes in time??!?!', () => {
            const { extensionHostAPI, extensionAPI } = initializeExtensionHostTest(
                {
                    initialSettings: initialSettings({ a: 'b' }),
                    clientApplication: 'sourcegraph',
                },
                pretendRemote<MainThreadAPI>({
                    getScriptURLForExtension: proxy(() => undefined),
                    getEnabledExtensions: () => proxySubscribable(new BehaviorSubject([])),
                })
            )

            const config = extensionAPI.configuration.get<{ a: string }>()
            expect(config.get('a')).toBe('b')
            extensionHostAPI.syncSettingsData({ subjects: [], final: { a: 'c' } })
            const newConfigSnapshot = extensionAPI.configuration.get<{ a: string }>()
            expect(newConfigSnapshot.get('a')).toBe('c')
        })
    })

    describe('talks to the client api', () => {
        test('talks to the client when an update is requested', async () => {
            const requestedEdits: SettingsEdit[] = []
            const { extensionAPI } = initializeExtensionHostTest(
                {
                    initialSettings: initialSettings({ a: 'b' }),
                    clientApplication: 'sourcegraph',
                },
                pretendRemote<MainThreadAPI>({
                    getScriptURLForExtension: proxy(() => undefined),
                    getEnabledExtensions: () => proxySubscribable(new BehaviorSubject([])),
                    applySettingsEdit: edit =>
                        Promise.resolve().then(() => {
                            requestedEdits.push(edit)
                        }),
                })
            )
            const config = extensionAPI.configuration.get<{ a: string }>()
            await config.update('a', 'aha!')
            expect(requestedEdits).toEqual<SettingsEdit[]>([{ path: ['a'], value: 'aha!' }])
            expect(config.get('a')).toBe('b') // no optimistic updates
        })
    })
})
