import classNames from 'classnames'
import { debounce } from 'lodash'
import CheckIcon from 'mdi-react/CheckIcon'
import InformationOutlineIcon from 'mdi-react/InformationOutlineIcon'
import OpenInNewIcon from 'mdi-react/OpenInNewIcon'
import PencilIcon from 'mdi-react/PencilIcon'
import * as Monaco from 'monaco-editor'
import React, { useState, useMemo, useCallback } from 'react'
import { of } from 'rxjs'
import { startWith } from 'rxjs/operators'

import { Hoverifier } from '@sourcegraph/codeintellify'
import { isErrorLike } from '@sourcegraph/common'
import { ActionItemAction } from '@sourcegraph/shared/src/actions/ActionItem'
import { HoverMerged } from '@sourcegraph/shared/src/api/client/types/hover'
import { CodeExcerpt } from '@sourcegraph/shared/src/components/CodeExcerpt'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { HoverContext } from '@sourcegraph/shared/src/hover/HoverOverlay'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { SymbolIcon } from '@sourcegraph/shared/src/symbols/SymbolIcon'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { toPrettyBlobURL } from '@sourcegraph/shared/src/util/url'
import { useCodeIntelViewerUpdates } from '@sourcegraph/shared/src/util/useCodeIntelViewerUpdates'
import { Alert, Icon, Link, LoadingSpinner, useObservable } from '@sourcegraph/wildcard'

import { BlockProps, SymbolBlock, SymbolBlockInput, SymbolBlockOutput } from '../..'
import { BlockMenuAction } from '../menu/NotebookBlockMenu'
import { useCommonBlockMenuActions } from '../menu/useCommonBlockMenuActions'
import { NotebookBlock } from '../NotebookBlock'
import { useModifierKeyLabel } from '../useModifierKeyLabel'

import styles from './NotebookSymbolBlock.module.scss'
import { NotebookSymbolBlockInput } from './NotebookSymbolBlockInput'

interface NotebookSymbolBlockProps
    extends BlockProps<SymbolBlock>,
        ThemeProps,
        TelemetryProps,
        PlatformContextProps<'requestGraphQL' | 'urlToFile' | 'settings' | 'forceUpdateTooltip'>,
        ExtensionsControllerProps<'extHostAPI' | 'executeCommand'> {
    sourcegraphSearchLanguageId: string
    hoverifier: Hoverifier<HoverContext, HoverMerged, ActionItemAction>
}

const LOADING = 'LOADING' as const

function isSymbolOutputLoaded(
    output: SymbolBlockOutput | Error | typeof LOADING | undefined
): output is SymbolBlockOutput {
    return output !== undefined && !isErrorLike(output) && output !== LOADING
}

export const NotebookSymbolBlock: React.FunctionComponent<NotebookSymbolBlockProps> = ({
    id,
    input,
    output,
    telemetryService,
    isSelected,
    isOtherBlockSelected,
    isReadOnly,
    hoverifier,
    extensionsController,
    isLightTheme,
    onRunBlock,
    onSelectBlock,
    onBlockInputChange,
    ...props
}) => {
    const [editor, setEditor] = useState<Monaco.editor.IStandaloneCodeEditor>()
    const [showInputs, setShowInputs] = useState(input.symbolName.length === 0)
    const [symbolQueryInput, setSymbolQueryInput] = useState('')
    const [isInputFocused, setIsInputFocused] = useState(false)

    const debouncedSetSymbolQueryInput = useMemo(() => debounce(setSymbolQueryInput, 300), [setSymbolQueryInput])
    const onSymbolSelected = useCallback(
        (input: SymbolBlockInput) => {
            onBlockInputChange(id, { type: 'symbol', input })
            onRunBlock(id)
        },
        [id, onBlockInputChange, onRunBlock]
    )

    const onEnterBlock = useCallback(() => {
        if (showInputs) {
            // setTimeout executes the editor focus in a separate run-loop which prevents adding a newline at the start of the input
            setTimeout(() => editor?.focus(), 0)
        } else if (!isReadOnly) {
            setShowInputs(true)
        }
    }, [editor, showInputs, isReadOnly, setShowInputs])

    const hideInputs = useCallback(() => setShowInputs(false), [setShowInputs])

    const symbolOutput = useObservable(useMemo(() => output?.pipe(startWith(LOADING)) ?? of(undefined), [output]))

    const commonMenuActions = useCommonBlockMenuActions({
        isInputFocused,
        isReadOnly,
        ...props,
    })

    const symbolURL = useMemo(
        () =>
            symbolOutput && symbolOutput !== LOADING && !isErrorLike(symbolOutput)
                ? toPrettyBlobURL({
                      repoName: input.repositoryName,
                      revision: symbolOutput.effectiveRevision,
                      filePath: input.filePath,
                      range: symbolOutput.symbolRange,
                  })
                : '',
        [input, symbolOutput]
    )

    const linkMenuAction: BlockMenuAction[] = useMemo(
        () => [
            {
                type: 'link',
                label: 'Open in new tab',
                icon: <Icon as={OpenInNewIcon} className="icon-inline" />,
                url: symbolURL,
                isDisabled: symbolURL.length === 0,
            },
        ],
        [symbolURL]
    )

    const modifierKeyLabel = useModifierKeyLabel()
    const toggleEditMenuAction: BlockMenuAction[] = useMemo(
        () => [
            {
                type: 'button',
                label: showInputs ? 'Save' : 'Edit',
                icon: showInputs ? (
                    <Icon as={CheckIcon} className="icon-inline" />
                ) : (
                    <Icon as={PencilIcon} className="icon-inline" />
                ),
                onClick: () => setShowInputs(!showInputs),
                keyboardShortcutLabel: showInputs ? `${modifierKeyLabel} + ↵` : '↵',
            },
        ],
        [modifierKeyLabel, showInputs, setShowInputs]
    )

    const menuActions = useMemo(
        () => (!isReadOnly ? toggleEditMenuAction : []).concat(linkMenuAction).concat(commonMenuActions),
        [isReadOnly, linkMenuAction, toggleEditMenuAction, commonMenuActions]
    )

    const codeIntelViewerUpdatesProps = useMemo(
        () => ({
            extensionsController,
            ...input,
            revision: isSymbolOutputLoaded(symbolOutput) ? symbolOutput.effectiveRevision : input.revision,
        }),
        [symbolOutput, extensionsController, input]
    )

    const viewerUpdates = useCodeIntelViewerUpdates(codeIntelViewerUpdatesProps)

    return (
        <NotebookBlock
            className={styles.block}
            id={id}
            isReadOnly={isReadOnly}
            isInputFocused={isInputFocused}
            aria-label="Notebook symbol block"
            onEnterBlock={onEnterBlock}
            isSelected={isSelected}
            isOtherBlockSelected={isOtherBlockSelected}
            onRunBlock={hideInputs}
            onBlockInputChange={onBlockInputChange}
            onSelectBlock={onSelectBlock}
            actions={isSelected ? menuActions : linkMenuAction}
            {...props}
        >
            <div className={styles.header}>
                {input.symbolName.length > 0 ? (
                    <NotebookSymbolBlockHeader
                        {...input}
                        symbolFoundAtLatestRevision={
                            isSymbolOutputLoaded(symbolOutput) ? symbolOutput.symbolFoundAtLatestRevision : undefined
                        }
                        effectiveRevision={
                            isSymbolOutputLoaded(symbolOutput) ? symbolOutput.effectiveRevision.slice(0, 7) : ''
                        }
                        symbolURL={symbolURL}
                    />
                ) : (
                    <>No symbol selected</>
                )}
            </div>
            {showInputs && (
                <NotebookSymbolBlockInput
                    id={id}
                    editor={editor}
                    symbolQueryInput={symbolQueryInput}
                    isLightTheme={isLightTheme}
                    setEditor={setEditor}
                    setSymbolQueryInput={setSymbolQueryInput}
                    debouncedSetSymbolQueryInput={debouncedSetSymbolQueryInput}
                    onSymbolSelected={onSymbolSelected}
                    setIsInputFocused={setIsInputFocused}
                    onRunBlock={hideInputs}
                    onSelectBlock={onSelectBlock}
                    {...props}
                />
            )}
            {symbolOutput === LOADING && (
                <div className={classNames('d-flex justify-content-center py-3', styles.highlightedFileWrapper)}>
                    <LoadingSpinner inline={false} />
                </div>
            )}
            {isSymbolOutputLoaded(symbolOutput) && (
                <div className={styles.highlightedFileWrapper}>
                    <CodeExcerpt
                        repoName={input.repositoryName}
                        commitID={input.revision}
                        filePath={input.filePath}
                        blobLines={symbolOutput.highlightedLines}
                        highlightRanges={[symbolOutput.highlightSymbolRange]}
                        {...symbolOutput.highlightLineRange}
                        isFirst={false}
                        fetchHighlightedFileRangeLines={() => of([])}
                        hoverifier={hoverifier}
                        viewerUpdates={viewerUpdates}
                    />
                </div>
            )}
            {symbolOutput && symbolOutput !== LOADING && isErrorLike(symbolOutput) && (
                <Alert className="m-3" variant="danger">
                    {symbolOutput.message}
                </Alert>
            )}
        </NotebookBlock>
    )
}

interface NotebookSymbolBlockHeaderProps extends SymbolBlockInput {
    symbolFoundAtLatestRevision: boolean | undefined
    effectiveRevision: string
    symbolURL: string
}

const NotebookSymbolBlockHeader: React.FunctionComponent<NotebookSymbolBlockHeaderProps> = ({
    repositoryName,
    filePath,
    symbolFoundAtLatestRevision,
    effectiveRevision,
    symbolName,
    symbolContainerName,
    symbolKind,
    symbolURL,
}) => (
    <>
        <div className="mr-2">
            <SymbolIcon className="icon-inline" kind={symbolKind} />
        </div>
        <div className="d-flex flex-column">
            <div className="mb-1 d-flex align-items-center">
                <code data-testid="selected-symbol-name">
                    <Link className={styles.headerLink} to={symbolURL}>
                        {symbolName}
                    </Link>
                    {symbolContainerName && <span className="text-muted"> {symbolContainerName}</span>}
                </code>
                {symbolFoundAtLatestRevision === false && (
                    <Icon
                        as={InformationOutlineIcon}
                        className="icon-inline ml-1"
                        data-tooltip={`Symbol not found at the latest revision, showing symbol at revision ${effectiveRevision}.`}
                    />
                )}
            </div>
            <small className="text-muted">
                {repositoryName}/{filePath}
            </small>
        </div>
    </>
)
