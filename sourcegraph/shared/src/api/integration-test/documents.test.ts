import { TextDocument } from 'sourcegraph'
import { collectSubscribableValues, integrationTestContext } from './testHelpers'

describe('Documents (integration)', () => {
    describe('workspace.textDocuments', () => {
        test('lists text documents', async () => {
            const { extensionHost } = await integrationTestContext()
            expect(extensionHost.workspace.textDocuments).toEqual([
                { uri: 'file:///f', languageId: 'l', text: 't' },
            ] as TextDocument[])
        })

        test('adds new text documents', async () => {
            const { model, extensionHost } = await integrationTestContext()
            model.next({
                ...model.value,
                visibleViewComponents: [
                    {
                        type: 'textEditor',
                        item: { uri: 'file:///f2', languageId: 'l2', text: 't2' },
                        selections: [],
                        isActive: true,
                    },
                ],
            })
            await extensionHost.internal.sync()
            expect(extensionHost.workspace.textDocuments).toEqual([
                { uri: 'file:///f', languageId: 'l', text: 't' },
                { uri: 'file:///f2', languageId: 'l2', text: 't2' },
            ] as TextDocument[])
        })
    })

    describe('workspace.openedTextDocuments', () => {
        test('fires when a text document is opened', async () => {
            const { model, extensionHost } = await integrationTestContext()

            const values = collectSubscribableValues(extensionHost.workspace.openedTextDocuments)
            expect(values).toEqual([] as TextDocument[])

            model.next({
                ...model.value,
                visibleViewComponents: [
                    {
                        type: 'textEditor',
                        item: { uri: 'file:///f2', languageId: 'l2', text: 't2' },
                        selections: [],
                        isActive: true,
                    },
                ],
            })
            await extensionHost.internal.sync()

            expect(values).toEqual([{ uri: 'file:///f2', languageId: 'l2', text: 't2' }] as TextDocument[])
        })
    })
})
