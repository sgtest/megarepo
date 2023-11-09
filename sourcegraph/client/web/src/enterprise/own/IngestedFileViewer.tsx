import { useMemo } from 'react'

import { EditorState, type Extension } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { useLocation, useNavigate } from 'react-router-dom'

import {
    addLineRangeQueryParameter,
    formatSearchParameters,
    toPositionOrRangeQueryParameter,
} from '@sourcegraph/common'
import { CodeMirrorEditor, defaultEditorTheme } from '@sourcegraph/shared/src/components/CodeMirrorEditor'
import { parseQueryAndHash } from '@sourcegraph/shared/src/util/url'

import { selectableLineNumbers } from '../../repo/blob/codemirror/linenumbers'

const theme = EditorView.theme({
    '.selected-line, .cm-line.selected-line': {
        backgroundColor: 'var(--code-selection-bg)',
    },
    '.cm-scroller': {
        borderRadius: 'var(--border-radius)',
    },
})

export const IngestedFileViewer: React.FunctionComponent<{ contents: string }> = ({ contents }) => {
    const location = useLocation()
    const navigate = useNavigate()
    const search = location.search

    const lineNumber = useMemo(
        () => parseQueryAndHash(location.search, location.hash).line,
        [location.search, location.hash]
    )

    const extensions: Extension[] = useMemo(
        () => [
            EditorState.readOnly.of(true),
            theme,
            selectableLineNumbers({
                onSelection(range) {
                    let query
                    if (range) {
                        const position = { line: range.line }
                        query = toPositionOrRangeQueryParameter(
                            range.endLine ? { range: { start: position, end: { line: range.endLine } } } : { position }
                        )
                    }
                    const newSearchParameters = addLineRangeQueryParameter(new URLSearchParams(search), query)
                    navigate('?' + formatSearchParameters(newSearchParameters))
                },
                initialSelection: lineNumber ? { line: lineNumber } : null,
            }),
            defaultEditorTheme,
        ],
        [lineNumber, search, navigate]
    )

    return <CodeMirrorEditor value={contents} extensions={extensions} />
}
