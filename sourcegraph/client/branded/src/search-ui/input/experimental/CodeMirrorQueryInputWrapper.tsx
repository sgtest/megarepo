import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { defaultKeymap, historyKeymap, history as codemirrorHistory } from '@codemirror/commands'
import { Compartment, EditorState, Extension, Prec } from '@codemirror/state'
import { EditorView, keymap, drawSelection } from '@codemirror/view'
import inRange from 'lodash/inRange'
import { useNavigate } from 'react-router-dom-v5-compat'
import useResizeObserver from 'use-resize-observer'
import * as uuid from 'uuid'

import { HistoryOrNavigate } from '@sourcegraph/common'
import { SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'
import { Shortcut } from '@sourcegraph/shared/src/react-shortcuts'
import { QueryChangeSource, QueryState } from '@sourcegraph/shared/src/search'
import { getTokenLength } from '@sourcegraph/shared/src/search/query/utils'

import { singleLine, placeholder as placeholderExtension } from '../codemirror'
import { parseInputAsQuery, tokens } from '../codemirror/parsedQuery'
import { querySyntaxHighlighting } from '../codemirror/syntax-highlighting'

import { filterHighlight } from './codemirror/syntax-highlighting'
import { modeScope } from './modes'
import { editorConfigFacet, Source, suggestions } from './suggestionsExtension'

import styles from './CodeMirrorQueryInputWrapper.module.scss'

interface ExtensionConfig {
    popoverID: string
    isLightTheme: boolean
    placeholder: string
    onChange: (querySate: QueryState) => void
    onSubmit?: () => void
    suggestionsContainer: HTMLDivElement | null
    suggestionSource?: Source
    historyOrNavigate: HistoryOrNavigate
}

// We want to show a placeholder also if the query only contains a context
// filter.
function showWhenEmptyWithoutContext(state: EditorState): boolean {
    // Show placeholder when empty
    if (state.doc.length === 0) {
        return true
    }

    const queryTokens = tokens(state)

    if (queryTokens.length > 2) {
        return false
    }
    // Only show the placeholder if the cursor is at the end of the content
    if (state.selection.main.from !== state.doc.length) {
        return false
    }

    // If there are two tokens, only show the placeholder if the second one is a
    // whitespace of length 1
    if (queryTokens.length === 2 && (queryTokens[1].type !== 'whitespace' || getTokenLength(queryTokens[1]) !== 1)) {
        return false
    }

    return (
        queryTokens.length > 0 &&
        queryTokens[0].type === 'filter' &&
        queryTokens[0].field.value === 'context' &&
        !inRange(state.selection.main.from, queryTokens[0].range.start, queryTokens[0].range.end + 1)
    )
}

// For simplicity we will recompute all extensions when input changes using
// this compartment
const extensionsCompartment = new Compartment()

// Helper function to update extensions dependent on props. Used when
// creating the editor and to update it when the props change.
function configureExtensions({
    popoverID,
    isLightTheme,
    placeholder,
    onChange,
    onSubmit,
    suggestionsContainer,
    suggestionSource,
    historyOrNavigate,
}: ExtensionConfig): Extension {
    const extensions = [
        singleLine,
        EditorView.darkTheme.of(isLightTheme === false),
        EditorView.updateListener.of(update => {
            if (update.docChanged) {
                onChange({
                    query: update.state.sliceDoc(),
                    changeSource: QueryChangeSource.userInput,
                })
            }
        }),
    ]

    if (placeholder) {
        extensions.push(placeholderExtension(placeholder, showWhenEmptyWithoutContext))
    }

    if (onSubmit) {
        extensions.push(
            editorConfigFacet.of({ onSubmit }),
            Prec.high(
                keymap.of([
                    {
                        key: 'Enter',
                        run() {
                            onSubmit()
                            return true
                        },
                    },
                    {
                        key: 'Mod-Enter',
                        run() {
                            onSubmit()
                            return true
                        },
                    },
                ])
            )
        )
    }

    if (suggestionSource && suggestionsContainer) {
        extensions.push(
            suggestions({
                id: popoverID,
                parent: suggestionsContainer,
                source: suggestionSource,
                historyOrNavigate,
            })
        )
    }

    return extensions
}

// Holds extensions that somehow depend on the query or query parameters. They
// are stored in a separate compartment to avoid re-creating other extensions.
// (if we didn't do this the suggestions list would flicker because it gets
// recreated)
const querySettingsCompartment = new Compartment()

function configureQueryExtensions({
    patternType,
    interpretComments,
}: {
    patternType: SearchPatternType
    interpretComments: boolean
}): Extension {
    return parseInputAsQuery({ patternType, interpretComments })
}

function createEditor(
    parent: HTMLDivElement,
    popoverID: string,
    queryState: QueryState,
    extensions: Extension,
    queryExtensions: Extension
): EditorView {
    return new EditorView({
        state: EditorState.create({
            doc: queryState.query,
            selection: { anchor: queryState.query.length },
            extensions: [
                drawSelection(),
                EditorView.lineWrapping,
                EditorView.contentAttributes.of({
                    role: 'combobox',
                    'aria-controls': popoverID,
                    'aria-owns': popoverID,
                    'aria-haspopup': 'grid',
                }),
                keymap.of(historyKeymap),
                keymap.of(defaultKeymap),
                codemirrorHistory(),
                Prec.low([querySyntaxHighlighting, modeScope(filterHighlight, [null])]),
                EditorView.theme({
                    '&': {
                        flex: 1,
                        backgroundColor: 'var(--input-bg)',
                        borderRadius: 'var(--border-radius)',
                        borderColor: 'var(--border-color)',
                    },
                    '&.cm-editor.cm-focused': {
                        outline: 'none',
                    },
                    '.cm-content': {
                        caretColor: 'var(--search-query-text-color)',
                        fontFamily: 'var(--code-font-family)',
                        fontSize: 'var(--code-font-size)',
                        color: 'var(--search-query-text-color)',
                        padding: 0,
                        paddingLeft: '0.25rem',
                    },
                    '.cm-line': {
                        padding: 0,
                    },
                }),
                querySettingsCompartment.of(queryExtensions),
                extensionsCompartment.of(extensions),
            ],
        }),
        parent,
    })
}

function updateExtensions(editor: EditorView | null, extensions: Extension): void {
    if (editor) {
        editor.dispatch({ effects: extensionsCompartment.reconfigure(extensions) })
    }
}

function updateQueryExtensions(editor: EditorView | null, extensions: Extension): void {
    if (editor) {
        editor.dispatch({ effects: querySettingsCompartment.reconfigure(extensions) })
    }
}

function updateValueIfNecessary(editor: EditorView | null, queryState: QueryState): void {
    if (editor && queryState.changeSource !== QueryChangeSource.userInput) {
        editor.dispatch({
            changes: { from: 0, to: editor.state.doc.length, insert: queryState.query },
            selection: { anchor: queryState.query.length },
        })
    }
}

const empty: any[] = []

export interface CodeMirrorQueryInputWrapperProps {
    queryState: QueryState
    onChange: (queryState: QueryState) => void
    onSubmit: () => void
    isLightTheme: boolean
    interpretComments: boolean
    patternType: SearchPatternType
    placeholder: string
    suggestionSource?: Source
    extensions?: Extension
}

export const CodeMirrorQueryInputWrapper: React.FunctionComponent<
    React.PropsWithChildren<CodeMirrorQueryInputWrapperProps>
> = ({
    queryState,
    onChange,
    onSubmit,
    isLightTheme,
    interpretComments,
    patternType,
    placeholder,
    suggestionSource,
    extensions: externalExtensions = empty,
    children,
}) => {
    const navigate = useNavigate()
    const [container, setContainer] = useState<HTMLDivElement | null>(null)
    const focusContainerRef = useRef<HTMLDivElement | null>(null)
    const [suggestionsContainer, setSuggestionsContainer] = useState<HTMLDivElement | null>(null)
    const popoverID = useMemo(() => uuid.v4(), [])

    // Wraps the onSubmit prop because that one changes whenever the input
    // value changes causing unnecessary reconfiguration of the extensions
    const onSubmitRef = useRef(onSubmit)
    onSubmitRef.current = onSubmit
    const hasSubmitHandler = !!onSubmit

    // Update extensions whenever any of these props change
    const extensions = useMemo(
        () => [
            configureExtensions({
                popoverID,
                isLightTheme,
                placeholder,
                onChange,
                onSubmit: hasSubmitHandler ? (): void => onSubmitRef.current?.() : undefined,
                suggestionsContainer,
                suggestionSource,
                historyOrNavigate: navigate,
            }),
            externalExtensions,
        ],
        [
            popoverID,
            isLightTheme,
            placeholder,
            onChange,
            hasSubmitHandler,
            onSubmitRef,
            suggestionsContainer,
            suggestionSource,
            navigate,
            externalExtensions,
        ]
    )

    // Update query extensions whenever any of these props change
    const queryExtensions = useMemo(
        () => configureQueryExtensions({ patternType, interpretComments }),
        [patternType, interpretComments]
    )

    const editor = useMemo(
        () => (container ? createEditor(container, popoverID, queryState, extensions, queryExtensions) : null),
        // Should only run once when the component is created, not when
        // extensions for state update (this is handled in separate hooks)
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [container]
    )
    const editorRef = useRef(editor)
    editorRef.current = editor
    useEffect(() => () => editor?.destroy(), [editor])

    // Update editor content whenever query state changes
    useEffect(() => updateValueIfNecessary(editorRef.current, queryState), [queryState])

    // Update editor configuration whenever extensions change
    useEffect(() => updateExtensions(editorRef.current, extensions), [extensions])
    useEffect(() => updateQueryExtensions(editorRef.current, queryExtensions), [queryExtensions])

    const focus = useCallback(() => {
        editorRef.current?.contentDOM.focus()
    }, [editorRef])

    const { ref: spacerRef, height: spacerHeight } = useResizeObserver({
        ref: focusContainerRef,
    })

    return (
        <div className={styles.container}>
            {/* eslint-disable-next-line react/forbid-dom-props */}
            <div className={styles.spacer} style={{ height: `${spacerHeight}px` }} />
            <div className={styles.root}>
                <div ref={spacerRef} className={styles.focusContainer}>
                    <div ref={setContainer} className="d-contents" />
                    {children}
                </div>
                <div ref={setSuggestionsContainer} className={styles.suggestions} />
            </div>
            <Shortcut ordered={['/']} onMatch={focus} />
        </div>
    )
}
