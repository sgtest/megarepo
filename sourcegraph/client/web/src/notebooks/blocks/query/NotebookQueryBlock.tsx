import React, { useState, useCallback, useMemo, useEffect } from 'react'

import { EditorView } from '@codemirror/view'
import { mdiPlayCircleOutline, mdiOpenInNew, mdiMagnify } from '@mdi/js'
import classNames from 'classnames'
import { Observable, of } from 'rxjs'

import {
    StreamingSearchResultsList,
    CodeMirrorQueryInput,
    changeListener,
    createDefaultSuggestions,
} from '@sourcegraph/branded'
import { FetchFileParameters } from '@sourcegraph/shared/src/backend/file'
import { editorHeight } from '@sourcegraph/shared/src/components/CodeMirrorEditor'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { SearchContextProps } from '@sourcegraph/shared/src/search'
import { fetchStreamSuggestions } from '@sourcegraph/shared/src/search/suggestions'
import { SettingsCascadeProps, useExperimentalFeatures } from '@sourcegraph/shared/src/settings/settings'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { buildSearchURLQuery } from '@sourcegraph/shared/src/util/url'
import { LoadingSpinner, useObservable, Icon } from '@sourcegraph/wildcard'

import { BlockProps, QueryBlock } from '../..'
import { AuthenticatedUser } from '../../../auth'
import { useFeatureFlag } from '../../../featureFlags/useFeatureFlag'
import { SearchPatternType } from '../../../graphql-operations'
import { OwnConfigProps } from '../../../own/OwnConfigProps'
import { submitSearch } from '../../../search/helpers'
import { setSearchMode, useNavbarQueryState } from '../../../stores'
import { blockKeymap, focusEditor as focusCodeMirrorInput } from '../../codemirror-utils'
import { BlockMenuAction } from '../menu/NotebookBlockMenu'
import { useCommonBlockMenuActions } from '../menu/useCommonBlockMenuActions'
import { NotebookBlock } from '../NotebookBlock'
import { useModifierKeyLabel } from '../useModifierKeyLabel'

import styles from './NotebookQueryBlock.module.scss'

interface NotebookQueryBlockProps
    extends BlockProps<QueryBlock>,
        Pick<SearchContextProps, 'searchContextsEnabled'>,
        SettingsCascadeProps,
        TelemetryProps,
        PlatformContextProps<'requestGraphQL' | 'urlToFile' | 'settings'>,
        OwnConfigProps {
    isSourcegraphDotCom: boolean
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
    authenticatedUser: AuthenticatedUser | null
}

// Defines the max height for the CodeMirror editor
const maxEditorHeight = editorHeight({ maxHeight: '300px' })
const editorAttributes = [
    EditorView.editorAttributes.of({
        'data-testid': 'notebook-query-block-input',
    }),
    EditorView.contentAttributes.of({
        'aria-label': 'Search query input',
    }),
]

export const NotebookQueryBlock: React.FunctionComponent<React.PropsWithChildren<NotebookQueryBlockProps>> = React.memo(
    ({
        id,
        input,
        output,
        telemetryService,
        settingsCascade,
        isSelected,
        onBlockInputChange,
        fetchHighlightedFileLineRanges,
        onRunBlock,
        isSourcegraphDotCom,
        searchContextsEnabled,
        ownEnabled,
        ...props
    }) => {
        const [editor, setEditor] = useState<EditorView>()
        const searchResults = useObservable(output ?? of(undefined))
        const [executedQuery, setExecutedQuery] = useState<string>(input.query)
        const applySuggestionsOnEnter =
            useExperimentalFeatures(features => features.applySearchQuerySuggestionOnEnter) ?? true
        const [ownFeatureFlagEnabled] = useFeatureFlag('search-ownership', false)
        const enableOwnershipSearch = ownEnabled && ownFeatureFlagEnabled

        const caseSensitive = useNavbarQueryState(state => state.searchCaseSensitivity)
        const searchMode = useNavbarQueryState(state => state.searchMode)
        const submittedURLQuery = useNavbarQueryState(state => state.searchQueryFromURL)

        const onInputChange = useCallback(
            (query: string) => onBlockInputChange(id, { type: 'query', input: { query } }),
            [id, onBlockInputChange]
        )

        const runBlock = useCallback(() => onRunBlock(id), [id, onRunBlock])

        useEffect(() => {
            setExecutedQuery(input.query)
            // We intentionally want to track the input query state at the time
            // of search submission, not on input change.
            // eslint-disable-next-line react-hooks/exhaustive-deps
        }, [output])

        const modifierKeyLabel = useModifierKeyLabel()
        const mainMenuAction: BlockMenuAction = useMemo(() => {
            const isLoading = searchResults && searchResults.state === 'loading'
            return {
                type: 'button',
                label: isLoading ? 'Searching...' : 'Run search',
                isDisabled: isLoading ?? false,
                icon: <Icon aria-hidden={true} svgPath={mdiPlayCircleOutline} />,
                onClick: onRunBlock,
                keyboardShortcutLabel: isSelected ? `${modifierKeyLabel} + ↵` : '',
            }
        }, [onRunBlock, isSelected, modifierKeyLabel, searchResults])

        const linkMenuActions: BlockMenuAction[] = useMemo(
            () => [
                {
                    type: 'link',
                    label: 'Open in new tab',
                    icon: <Icon aria-hidden={true} svgPath={mdiOpenInNew} />,
                    url: `/search?${buildSearchURLQuery(input.query, SearchPatternType.standard, false)}`,
                },
            ],
            [input]
        )

        const commonMenuActions = linkMenuActions.concat(useCommonBlockMenuActions({ id, ...props }))

        const focusInput = useCallback(() => {
            if (editor) {
                focusCodeMirrorInput(editor)
            }
        }, [editor])

        const queryCompletion = useMemo(
            () =>
                createDefaultSuggestions({
                    isSourcegraphDotCom,
                    fetchSuggestions: fetchStreamSuggestions,
                    applyOnEnter: applySuggestionsOnEnter,
                }),
            [isSourcegraphDotCom, applySuggestionsOnEnter]
        )

        // Focus editor on component creation if necessary
        useEffect(() => {
            if (editor && input.initialFocusInput) {
                focusCodeMirrorInput(editor)
            }
        }, [input.initialFocusInput, editor])

        return (
            <NotebookBlock
                id={id}
                aria-label="Notebook query block"
                isSelected={isSelected}
                isInputVisible={true}
                focusInput={focusInput}
                mainAction={mainMenuAction}
                actions={isSelected ? commonMenuActions : linkMenuActions}
                {...props}
            >
                <div className={styles.content}>
                    <div className={styles.queryInputWrapper}>
                        <Icon
                            aria-hidden={true}
                            svgPath={mdiMagnify}
                            style={{
                                display: 'inline-block',
                            }}
                        />
                        <div className={styles.codeMirrorWrapper}>
                            <CodeMirrorQueryInput
                                value={input.query}
                                patternType={SearchPatternType.standard}
                                interpretComments={true}
                                onEditorCreated={setEditor}
                                extensions={useMemo(
                                    () => [
                                        EditorView.lineWrapping,
                                        queryCompletion,
                                        changeListener(onInputChange),
                                        blockKeymap({ runBlock }),
                                        maxEditorHeight,
                                        editorAttributes,
                                    ],
                                    [queryCompletion, runBlock, onInputChange]
                                )}
                            />
                        </div>
                    </div>
                    {searchResults && searchResults.state === 'loading' && (
                        <div className={classNames('d-flex justify-content-center py-3', styles.results)}>
                            <LoadingSpinner />
                        </div>
                    )}
                    {searchResults && searchResults.state !== 'loading' && (
                        <div className={styles.results}>
                            <StreamingSearchResultsList
                                isSourcegraphDotCom={isSourcegraphDotCom}
                                enableOwnershipSearch={enableOwnershipSearch}
                                searchContextsEnabled={searchContextsEnabled}
                                allExpanded={false}
                                results={searchResults}
                                fetchHighlightedFileLineRanges={fetchHighlightedFileLineRanges}
                                telemetryService={telemetryService}
                                settingsCascade={settingsCascade}
                                platformContext={props.platformContext}
                                openMatchesInNewTab={true}
                                executedQuery={executedQuery}
                                searchMode={searchMode}
                                setSearchMode={setSearchMode}
                                submitSearch={submitSearch}
                                caseSensitive={caseSensitive}
                                searchQueryFromURL={submittedURLQuery}
                            />
                        </div>
                    )}
                </div>
            </NotebookBlock>
        )
    }
)

NotebookQueryBlock.displayName = 'NotebookQueryBlock'
