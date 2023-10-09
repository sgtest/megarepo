import {
    type FC,
    forwardRef,
    type MutableRefObject,
    type PropsWithChildren,
    useCallback,
    useEffect,
    useId,
    useMemo,
    useRef,
    useState,
    useImperativeHandle,
} from 'react'

import { EditorSelection, EditorState, type Extension, Prec } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { mdiClockOutline } from '@mdi/js'
import classNames from 'classnames'
import inRange from 'lodash/inRange'
import { useNavigate } from 'react-router-dom'
import useResizeObserver from 'use-resize-observer'

import type { HistoryOrNavigate } from '@sourcegraph/common'
import { type Editor, useCompartment, viewToEditor } from '@sourcegraph/shared/src/components/CodeMirrorEditor'
import type { SearchPatternType } from '@sourcegraph/shared/src/graphql-operations'
import { Shortcut } from '@sourcegraph/shared/src/react-shortcuts'
import { QueryChangeSource, type QueryState } from '@sourcegraph/shared/src/search'
import { getTokenLength } from '@sourcegraph/shared/src/search/query/utils'
import { Button, Icon, Tooltip } from '@sourcegraph/wildcard'

import { BaseCodeMirrorQueryInput } from '../BaseCodeMirrorQueryInput'
import { placeholder as placeholderExtension } from '../codemirror'
import { queryDiagnostic } from '../codemirror/diagnostics'
import { tokens } from '../codemirror/parsedQuery'
import { useUpdateInputFromQueryState } from '../codemirror/react'
import { tokenInfo } from '../codemirror/token-info'

import { overrideContextOnPaste } from './codemirror/searchcontext'
import { filterDecoration } from './codemirror/syntax-highlighting'
import { modeScope, useInputMode } from './modes'
import { type Source, suggestions, startCompletion } from './suggestionsExtension'

import styles from './CodeMirrorQueryInputWrapper.module.scss'

interface ExtensionConfig {
    popoverID: string
    placeholder: string
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

// Helper function to update extensions dependent on props. Used when
// creating the editor and to update it when the props change.
function configureExtensions({
    popoverID,
    placeholder,
    suggestionsContainer,
    suggestionSource,
    historyOrNavigate,
}: ExtensionConfig): Extension {
    const extensions = []

    if (placeholder) {
        extensions.push(placeholderExtension(placeholder, showWhenEmptyWithoutContext))
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

// Creates extensions that don't depend on props
const position0 = EditorSelection.single(0)
const staticExtensions: Extension = [
    EditorState.transactionFilter.of(transaction => {
        // This is a hacky way to "fix" the cursor position when the input receives
        // focus by clicking outside of it in Chrome.
        // Debugging has revealed that in such a case the transaction has a user event
        // 'select', the new selection is set to `0` and 'scrollIntoView' is 'false'.
        // This is different from other events that change the cursor position:
        // - Clicking on text inside the input (whether focused or not) will be a 'select.pointer'
        //   user event.
        // - Moving the cursor with arrow keys will be a 'select' user event but will also set
        //   'scrollIntoView' to 'true'
        // - Entering new characters will be of user type 'input'
        // - Selecting a text range will be of user type 'select.pointer'
        // - Tabbing to the input seems to only trigger a 'select' user event transaction when
        //   the user clicked outside the input (also only in Chrome, this transaction doesn't
        //   occur in Firefox)

        if (
            !transaction.isUserEvent('select.pointer') &&
            transaction.isUserEvent('select') &&
            !transaction.scrollIntoView &&
            transaction.selection?.eq(position0)
        ) {
            return [transaction, { selection: EditorSelection.single(transaction.newDoc.length) }]
        }
        return transaction
    }),
    modeScope([queryDiagnostic(), overrideContextOnPaste], [null]),
    Prec.low([modeScope([tokenInfo(), filterDecoration], [null])]),
    EditorView.theme({
        '&': {
            flex: 1,
            backgroundColor: 'var(--input-bg)',
            borderRadius: 'var(--border-radius)',
            borderColor: 'var(--border-color)',
            // To ensure that the input doesn't overflow the parent
            minWidth: 0,
            marginRight: '0.5rem',
        },
        '&.cm-editor.cm-focused': {
            outline: 'none',
        },
        '.cm-scroller': {
            overflowX: 'hidden',
        },
        '.cm-content': {
            paddingLeft: '0.25rem',
        },
        '.cm-content.focus-visible': {
            boxShadow: 'none',
        },
        '.sg-decorated-token-hover': {
            borderRadius: '3px',
        },
        '.sg-query-filter-placeholder': {
            color: 'var(--text-muted)',
            fontStyle: 'italic',
        },
    }),
]

export enum QueryInputVisualMode {
    Standard = 'standard',
    Compact = 'compact',
}

export interface CodeMirrorQueryInputWrapperProps {
    queryState: QueryState
    onChange: (queryState: QueryState) => void
    onSubmit: () => void
    interpretComments: boolean
    patternType: SearchPatternType
    placeholder: string
    suggestionSource?: Source
    extensions?: Extension
    visualMode?: QueryInputVisualMode | `${QueryInputVisualMode}`
    className?: string
}

export const CodeMirrorQueryInputWrapper = forwardRef<Editor, PropsWithChildren<CodeMirrorQueryInputWrapperProps>>(
    (
        {
            queryState,
            onChange,
            onSubmit,
            interpretComments,
            patternType,
            placeholder,
            suggestionSource,
            extensions: externalExtensions,
            visualMode = QueryInputVisualMode.Standard,
            className,
            children,
        },
        ref
    ) => {
        // Global params
        const popoverID = useId()
        const navigate = useNavigate()

        // References
        const editorRef = useRef<EditorView | null>(null)
        useImperativeHandle(ref, () => viewToEditor(editorRef))

        // Local state
        const [mode, setMode, modeNotifierExtension] = useInputMode()
        const [suggestionsContainer, setSuggestionsContainer] = useState<HTMLDivElement | null>(null)

        // Handlers
        const onSubmitRef = useMutableValue(onSubmit)
        const onChangeRef = useMutableValue(onChange)

        const onSubmitHandler = useCallback(
            (view: EditorView): boolean => {
                if (onSubmitRef.current) {
                    onSubmitRef.current()
                    view.contentDOM.blur()
                    return true
                }
                return false
            },
            [onSubmitRef]
        )

        const onChangeHandler = useCallback(
            (value: string) => onChangeRef.current?.({ query: value, changeSource: QueryChangeSource.userInput }),
            [onChangeRef]
        )

        // Update extensions whenever any of these props change
        const dynamicExtensions = useCompartment(
            editorRef,
            useMemo(
                () =>
                    configureExtensions({
                        popoverID,
                        placeholder,
                        suggestionsContainer,
                        suggestionSource,
                        historyOrNavigate: navigate,
                    }),
                [popoverID, placeholder, suggestionsContainer, suggestionSource, navigate]
            )
        )

        // Update editor state whenever query state changes
        useUpdateInputFromQueryState(editorRef, queryState, startCompletion)

        const allExtensions = useMemo(
            () => [
                externalExtensions ?? [],
                dynamicExtensions,
                modeNotifierExtension,
                EditorView.contentAttributes.of({
                    role: 'combobox',
                    // CodeMirror sets aria-multiline: true by default but it seems
                    // comboboxes are not allowed to be multiline
                    'aria-multiline': 'false',
                    'aria-controls': popoverID,
                    'aria-haspopup': 'grid',
                    'aria-label': 'Search query',
                }),
                staticExtensions,
            ],
            [popoverID, dynamicExtensions, externalExtensions, modeNotifierExtension]
        )

        // Position cursor at the end of the input when the input changes from external sources.
        // This is necessary because the initial value might be set asynchronously.
        useEffect(() => {
            const editor = editorRef.current
            if (editor && !editor.hasFocus && queryState.changeSource !== QueryChangeSource.userInput) {
                editor.dispatch({
                    selection: { anchor: editor.state.doc.length },
                })
            }
        }, [queryState])

        const focus = useCallback(() => {
            editorRef.current?.focus()
        }, [])

        const toggleHistoryMode = useCallback(() => {
            if (editorRef.current) {
                setMode(editorRef.current, mode => (mode === 'History' ? null : 'History'))
                editorRef.current.focus()
            }
        }, [setMode])

        const { ref: inputContainerRef, height = 0 } = useResizeObserver({ box: 'border-box' })

        return (
            <div
                ref={inputContainerRef}
                className={classNames(styles.container, className, 'test-experimental-search-input', 'test-editor', {
                    [styles.containerCompact]: visualMode === QueryInputVisualMode.Compact,
                })}
                role="search"
                data-editor="experimental-search-input"
            >
                <div className={styles.focusContainer}>
                    <SearchModeSwitcher mode={mode} onModeChange={toggleHistoryMode} />
                    <BaseCodeMirrorQueryInput
                        ref={editorRef}
                        className={styles.input}
                        value={queryState.query}
                        patternType={patternType}
                        interpretComments={interpretComments}
                        extension={allExtensions}
                        onEnter={onSubmitHandler}
                        onChange={onChangeHandler}
                        multiLine={false}
                    />
                    {!mode && children}
                </div>
                <div
                    ref={setSuggestionsContainer}
                    className={styles.suggestions}
                    // eslint-disable-next-line react/forbid-dom-props
                    style={{ paddingTop: height }}
                />
                <Shortcut ordered={['/']} onMatch={focus} />
            </div>
        )
    }
)
CodeMirrorQueryInputWrapper.displayName = 'CodeMirrorInputWrapper'

interface SearchModeSwitcherProps {
    mode: string | null
    className?: string
    onModeChange: () => void
}

const SearchModeSwitcher: FC<SearchModeSwitcherProps> = props => {
    const { mode, className, onModeChange } = props

    return (
        <div className={classNames(className, styles.mode, !!mode && styles.modeActive)}>
            <Tooltip content="Recent searches">
                <Button variant="icon" aria-label="Open search history" onClick={onModeChange}>
                    <Icon svgPath={mdiClockOutline} aria-hidden="true" />
                    {mode && <span className="ml-1">{mode}:</span>}
                </Button>
            </Tooltip>
        </div>
    )
}

function useMutableValue<T>(value: T): MutableRefObject<T> {
    const valueRef = useRef(value)

    useEffect(() => {
        valueRef.current = value
    }, [value])

    return valueRef
}
