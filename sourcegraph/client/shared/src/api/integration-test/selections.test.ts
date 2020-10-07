import { from } from 'rxjs'
import { distinctUntilChanged, filter, switchMap } from 'rxjs/operators'
import { isDefined, isTaggedUnionMember } from '../../util/types'
import { assertToJSON, collectSubscribableValues, integrationTestContext } from './testHelpers'

describe('Selections (integration)', () => {
    describe('editor.selectionsChanged', () => {
        test('reflects changes to the current selections', async () => {
            const {
                services: { viewer: viewerService },
                extensionAPI,
            } = await integrationTestContext()
            const editor = viewerService.viewers.get('viewer#0')!
            const selectionChanges = from(extensionAPI.app.activeWindowChanges).pipe(
                filter(isDefined),
                switchMap(window => window.activeViewComponentChanges),
                filter(isDefined),
                filter(isTaggedUnionMember('type', 'CodeEditor' as const)),
                distinctUntilChanged(),
                switchMap(editor => editor.selectionsChanges)
            )
            const selectionValues = collectSubscribableValues(selectionChanges)
            const testValues = [
                [{ start: 3, end: 5 }],
                [
                    { start: 1, end: 10 },
                    { start: 25, end: 40 },
                    { start: 56, end: 57 },
                ],
                [],
            ]
            for (const selections of testValues) {
                viewerService.setSelections(
                    editor,
                    selections.map(({ start, end }) => ({
                        start: {
                            line: start,
                            character: 0,
                        },
                        end: {
                            line: end,
                            character: 0,
                        },
                        anchor: {
                            line: start,
                            character: 0,
                        },
                        active: {
                            line: end,
                            character: 0,
                        },
                        isReversed: false,
                    }))
                )
                await extensionAPI.internal.sync()
            }
            assertToJSON(
                selectionValues.map(selections =>
                    selections.map(selection => ({ start: selection.start.line, end: selection.end.line }))
                ),
                [[], ...testValues]
            )
        })
    })
})
