import { WorkspaceRoot } from 'sourcegraph'

import { collectSubscribableValues, integrationTestContext } from './testHelpers'

describe('Workspace roots (integration)', () => {
    describe('workspace.roots', () => {
        test('lists roots', async () => {
            const { extensionAPI } = await integrationTestContext()
            expect(extensionAPI.workspace.roots).toEqual([{ uri: new URL('file:///') }] as WorkspaceRoot[])
        })

        test('adds new text documents', async () => {
            const { extensionAPI, extensionHostAPI } = await integrationTestContext()

            await Promise.all([
                extensionHostAPI.addWorkspaceRoot({
                    uri: 'file:///a',
                }),
                extensionHostAPI.addWorkspaceRoot({
                    uri: 'file:///b',
                }),
            ])

            expect(extensionAPI.workspace.roots).toEqual([
                { uri: new URL('file:///') },
                { uri: new URL('file:///a') },
                { uri: new URL('file:///b') },
            ] as WorkspaceRoot[])
        })
    })

    describe('workspace.rootChanges', () => {
        test('fires when a root is added or removed', async () => {
            const { extensionAPI, extensionHostAPI } = await integrationTestContext()

            const values = collectSubscribableValues(extensionAPI.workspace.rootChanges)
            expect(values).toEqual([] as void[])

            await extensionHostAPI.addWorkspaceRoot({
                uri: 'file:///a',
            })

            expect(values).toEqual([undefined])
        })
    })
})
