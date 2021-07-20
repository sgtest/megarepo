import classNames from 'classnames'
import { noop } from 'lodash'
import PlayCircleOutlineIcon from 'mdi-react/PlayCircleOutlineIcon'
import * as Monaco from 'monaco-editor'
import React, { useState, useCallback, useRef, useMemo } from 'react'
import { useLocation } from 'react-router'
import { Observable, of } from 'rxjs'

import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import { FetchFileParameters } from '@sourcegraph/shared/src/components/CodeExcerpt'
import { SearchPatternType } from '@sourcegraph/shared/src/graphql/schema'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'
import { MonacoEditor } from '@sourcegraph/web/src/components/MonacoEditor'

import { StreamingSearchResultsList } from '../results/StreamingSearchResultsList'
import { SOURCEGRAPH_SEARCH, useQueryDiagnostics } from '../useQueryIntelligence'

import blockStyles from './SearchNotebookBlock.module.scss'
import { SearchNotebookBlockMenu } from './SearchNotebookBlockMenu'
import styles from './SearchNotebookQueryBlock.module.scss'
import { useBlockSelection } from './useBlockSelection'
import { useBlockShortcuts } from './useBlockShortcuts'
import { useCommonBlockMenuActions } from './useCommonBlockMenuActions'
import { MONACO_BLOCK_INPUT_OPTIONS, useMonacoBlockInput } from './useMonacoBlockInput'

import { BlockProps, QueryBlock } from '.'

interface SearchNotebookQueryBlockProps
    extends BlockProps,
        Omit<QueryBlock, 'type'>,
        ThemeProps,
        SettingsCascadeProps,
        TelemetryProps {
    isMacPlatform: boolean
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

export const SearchNotebookQueryBlock: React.FunctionComponent<SearchNotebookQueryBlockProps> = ({
    id,
    input,
    output,
    isLightTheme,
    telemetryService,
    settingsCascade,
    isSelected,
    isMacPlatform,
    fetchHighlightedFileLineRanges,
    onRunBlock,
    ...props
}) => {
    const [editor, setEditor] = useState<Monaco.editor.IStandaloneCodeEditor>()
    const blockElement = useRef<HTMLDivElement>(null)
    const searchResults = useObservable(output ?? of(undefined))
    const location = useLocation()

    const { isInputFocused } = useMonacoBlockInput({ editor, id, onRunBlock, ...props })

    // setTimeout executes the editor focus in a separate run-loop which prevents adding a newline at the start of the input
    const onEnterBlock = useCallback(() => {
        setTimeout(() => editor?.focus(), 0)
    }, [editor])
    const { onSelect } = useBlockSelection({
        id,
        blockElement: blockElement.current,
        isSelected,
        isInputFocused,
        ...props,
    })
    const { onKeyDown } = useBlockShortcuts({ id, isMacPlatform, onEnterBlock, onRunBlock, ...props })

    const modifierKeyLabel = isMacPlatform ? '⌘' : 'Ctrl'
    const mainMenuAction = useMemo(() => {
        const isLoading = searchResults && searchResults.state === 'loading'
        return {
            label: isLoading ? 'Searching...' : 'Run search',
            isDisabled: isLoading ?? false,
            icon: <PlayCircleOutlineIcon className="icon-inline" />,
            onClick: onRunBlock,
            keyboardShortcutLabel: `${modifierKeyLabel} + ↵`,
        }
    }, [onRunBlock, modifierKeyLabel, searchResults])

    const commonMenuActions = useCommonBlockMenuActions({ modifierKeyLabel, isInputFocused, isMacPlatform, ...props })

    useQueryDiagnostics(editor, { patternType: SearchPatternType.literal, interpretComments: true })

    return (
        <div className={classNames('block-wrapper', blockStyles.blockWrapper)} data-block-id={id}>
            {/* Notebook blocks are a form of specialized UI for which there are no good accesibility settings (role, aria-*)
                or semantic elements that would accurately describe its functionality. To provide the necessary functionality we have
                to rely on plain div elements and custom click/focus/keyDown handlers. We still preserve the ability to navigate through blocks
                with the keyboard using the up and down arrows, and TAB. */}
            {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions */}
            <div
                className={classNames(
                    blockStyles.block,
                    styles.block,
                    isSelected && !isInputFocused && blockStyles.selected,
                    isSelected && isInputFocused && blockStyles.selectedNotFocused
                )}
                onClick={onSelect}
                onKeyDown={onKeyDown}
                onFocus={onSelect}
                // A tabIndex is necessary to make the block focusable.
                // eslint-disable-next-line jsx-a11y/no-noninteractive-tabindex
                tabIndex={0}
                aria-label="Notebook query block"
                ref={blockElement}
            >
                <div
                    className={classNames(
                        blockStyles.monacoWrapper,
                        isInputFocused && blockStyles.selected,
                        styles.queryInputMonacoWrapper
                    )}
                >
                    <MonacoEditor
                        language={SOURCEGRAPH_SEARCH}
                        value={input}
                        height="auto"
                        isLightTheme={isLightTheme}
                        editorWillMount={noop}
                        onEditorCreated={setEditor}
                        options={MONACO_BLOCK_INPUT_OPTIONS}
                        border={false}
                    />
                </div>

                {searchResults && searchResults.state === 'loading' && (
                    <div className={classNames('d-flex justify-content-center py-3', styles.results)}>
                        <LoadingSpinner />
                    </div>
                )}
                {searchResults && searchResults.state !== 'loading' && (
                    <div className={styles.results}>
                        <StreamingSearchResultsList
                            location={location}
                            allExpanded={false}
                            results={searchResults}
                            isLightTheme={isLightTheme}
                            fetchHighlightedFileLineRanges={fetchHighlightedFileLineRanges}
                            telemetryService={telemetryService}
                            settingsCascade={settingsCascade}
                        />
                    </div>
                )}
            </div>
            {isSelected && <SearchNotebookBlockMenu id={id} mainAction={mainMenuAction} actions={commonMenuActions} />}
        </div>
    )
}
