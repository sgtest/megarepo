import React, { useMemo, useState, useCallback } from 'react'

import { EditorView } from '@codemirror/view'
import { debounce } from 'lodash'
import InfoCircleOutlineIcon from 'mdi-react/InfoCircleOutlineIcon'

import { isMacPlatform as isMacPlatformFunc } from '@sourcegraph/common'
import { createDefaultSuggestions } from '@sourcegraph/search-ui'
import { IHighlightLineRange } from '@sourcegraph/shared/src/schema'
import { PathMatch } from '@sourcegraph/shared/src/search/stream'
import { fetchStreamSuggestions } from '@sourcegraph/shared/src/search/suggestions'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { Icon, Button, Input, InputStatus } from '@sourcegraph/wildcard'

import { BlockProps, FileBlockInput } from '../..'
import { parseLineRange, serializeLineRange } from '../../serialize'
import { SearchTypeSuggestionsInput } from '../suggestions/SearchTypeSuggestionsInput'
import { fetchSuggestions } from '../suggestions/suggestions'

import styles from './NotebookFileBlockInputs.module.scss'

interface NotebookFileBlockInputsProps extends Pick<BlockProps, 'onRunBlock'>, ThemeProps {
    id: string
    queryInput: string
    lineRange: IHighlightLineRange | null
    onEditorCreated: (editor: EditorView) => void
    setQueryInput: (value: string) => void
    onLineRangeChange: (lineRange: IHighlightLineRange | null) => void
    onFileSelected: (file: FileBlockInput) => void
    isSourcegraphDotCom: boolean
    globbing: boolean
}

function getFileSuggestionsQuery(queryInput: string): string {
    return `${queryInput} fork:yes type:path count:50`
}

const editorAttributes = [
    EditorView.editorAttributes.of({
        'data-testid': 'notebook-file-block-input',
    }),
    EditorView.contentAttributes.of({
        'aria-label': 'File search input',
    }),
]

export const NotebookFileBlockInputs: React.FunctionComponent<
    React.PropsWithChildren<NotebookFileBlockInputsProps>
> = ({ id, lineRange, onFileSelected, onLineRangeChange, globbing, isSourcegraphDotCom, ...inputProps }) => {
    const [lineRangeInput, setLineRangeInput] = useState(serializeLineRange(lineRange))
    const debouncedOnLineRangeChange = useMemo(() => debounce(onLineRangeChange, 300), [onLineRangeChange])

    const isLineRangeValid = useMemo(
        () => (lineRangeInput.trim() ? parseLineRange(lineRangeInput) !== null : undefined),
        [lineRangeInput]
    )

    const onLineRangeInputChange = useCallback(
        (event: React.ChangeEvent<HTMLInputElement>) => {
            setLineRangeInput(event.target.value)
            debouncedOnLineRangeChange(parseLineRange(event.target.value))
        },
        [setLineRangeInput, debouncedOnLineRangeChange]
    )

    const fetchFileSuggestions = useCallback(
        (query: string) =>
            fetchSuggestions(
                getFileSuggestionsQuery(query),
                (suggestion): suggestion is PathMatch => suggestion.type === 'path',
                file => file
            ),
        []
    )

    const countSuggestions = useCallback((suggestions: PathMatch[]) => suggestions.length, [])

    const onFileSuggestionSelected = useCallback(
        (file: FileBlockInput) => {
            onFileSelected(file)
            setLineRangeInput(serializeLineRange(file.lineRange))
        },
        [onFileSelected, setLineRangeInput]
    )

    const renderSuggestions = useCallback(
        (suggestions: PathMatch[]) => (
            <FileSuggestions suggestions={suggestions} onFileSelected={onFileSuggestionSelected} />
        ),
        [onFileSuggestionSelected]
    )

    const isMacPlatform = useMemo(() => isMacPlatformFunc(), [])

    const queryCompletion = useMemo(
        () =>
            createDefaultSuggestions({
                isSourcegraphDotCom,
                globbing,
                fetchSuggestions: fetchStreamSuggestions,
            }),
        [isSourcegraphDotCom, globbing]
    )

    return (
        <div className={styles.fileBlockInputs}>
            <div className="text-muted mb-2">
                <small>
                    <Icon aria-hidden={true} as={InfoCircleOutlineIcon} /> To automatically select a file, copy a
                    Sourcegraph file URL, select the block, and paste the URL ({isMacPlatform ? '⌘' : 'Ctrl'} + v).
                </small>
            </div>
            <SearchTypeSuggestionsInput<PathMatch>
                id={id}
                label="Find a file using a Sourcegraph search query"
                queryPrefix="type:path"
                fetchSuggestions={fetchFileSuggestions}
                countSuggestions={countSuggestions}
                renderSuggestions={renderSuggestions}
                extension={useMemo(() => [queryCompletion, editorAttributes], [queryCompletion])}
                {...inputProps}
            />
            <div className="mt-2">
                <Input
                    id={`${id}-line-range-input`}
                    status={InputStatus[isLineRangeValid === false ? 'error' : 'initial']}
                    value={lineRangeInput}
                    onChange={onLineRangeInputChange}
                    placeholder="Enter a single line (1), a line range (1-10), or leave empty to show the entire file."
                    label="Line range"
                    className="mb-0"
                    error={
                        isLineRangeValid === false &&
                        'Line range is invalid. Enter a single line (1), a line range (1-10), or leave empty to show the entire file.'
                    }
                />
            </div>
        </div>
    )
}

const FileSuggestions: React.FunctionComponent<
    React.PropsWithChildren<{
        suggestions: PathMatch[]
        onFileSelected: (symbol: FileBlockInput) => void
    }>
> = ({ suggestions, onFileSelected }) => (
    <div className={styles.fileSuggestions}>
        {suggestions.map(suggestion => (
            <Button
                className={styles.fileButton}
                key={`${suggestion.repository}_${suggestion.path}`}
                onClick={() =>
                    onFileSelected({
                        repositoryName: suggestion.repository,
                        filePath: suggestion.path,
                        revision: suggestion.commit ?? '',
                        lineRange: null,
                    })
                }
                data-testid="file-suggestion-button"
            >
                <span className="mb-1">{suggestion.path}</span>
                <small className="text-muted">{suggestion.repository}</small>
            </Button>
        ))}
    </div>
)
