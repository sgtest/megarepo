import React, { useState, useMemo, useCallback } from 'react'

import { EditorView } from '@codemirror/view'
import { mdiOpenInNew, mdiInformationOutline, mdiCheck, mdiPencil } from '@mdi/js'
import classNames from 'classnames'
import { debounce } from 'lodash'
import { of } from 'rxjs'
import { startWith } from 'rxjs/operators'

import { HoverMerged } from '@sourcegraph/client-api'
import { Hoverifier } from '@sourcegraph/codeintellify'
import { isErrorLike } from '@sourcegraph/common'
import { CodeExcerpt } from '@sourcegraph/search-ui'
import { ActionItemAction } from '@sourcegraph/shared/src/actions/ActionItem'
import { ExtensionsControllerProps } from '@sourcegraph/shared/src/extensions/controller'
import { HoverContext } from '@sourcegraph/shared/src/hover/HoverOverlay'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { getRepositoryUrl } from '@sourcegraph/shared/src/search/stream'
import { SymbolIcon } from '@sourcegraph/shared/src/symbols/SymbolIcon'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { codeCopiedEvent } from '@sourcegraph/shared/src/tracking/event-log-creators'
import { toPrettyBlobURL } from '@sourcegraph/shared/src/util/url'
import { useCodeIntelViewerUpdates } from '@sourcegraph/shared/src/util/useCodeIntelViewerUpdates'
import { Alert, Icon, LoadingSpinner, Tooltip, useObservable } from '@sourcegraph/wildcard'

import { BlockProps, SymbolBlock, SymbolBlockInput, SymbolBlockOutput } from '../..'
import { focusEditor } from '../../codemirror-utils'
import { BlockMenuAction } from '../menu/NotebookBlockMenu'
import { useCommonBlockMenuActions } from '../menu/useCommonBlockMenuActions'
import { NotebookBlock } from '../NotebookBlock'
import { RepoFileSymbolLink } from '../RepoFileSymbolLink'
import { useModifierKeyLabel } from '../useModifierKeyLabel'

import { NotebookSymbolBlockInput } from './NotebookSymbolBlockInput'

import styles from './NotebookSymbolBlock.module.scss'

interface NotebookSymbolBlockProps
    extends BlockProps<SymbolBlock>,
        ThemeProps,
        TelemetryProps,
        PlatformContextProps<'requestGraphQL' | 'urlToFile' | 'settings' | 'forceUpdateTooltip'>,
        ExtensionsControllerProps<'extHostAPI' | 'executeCommand'> {
    hoverifier: Hoverifier<HoverContext, HoverMerged, ActionItemAction>
    isSourcegraphDotCom: boolean
    globbing: boolean
}

const LOADING = 'LOADING' as const

function isSymbolOutputLoaded(
    output: SymbolBlockOutput | Error | typeof LOADING | undefined
): output is SymbolBlockOutput {
    return output !== undefined && !isErrorLike(output) && output !== LOADING
}

export const NotebookSymbolBlock: React.FunctionComponent<
    React.PropsWithChildren<NotebookSymbolBlockProps>
> = React.memo(
    ({
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
        onBlockInputChange,
        ...props
    }) => {
        const [editor, setEditor] = useState<EditorView | null>()
        const [showInputs, setShowInputs] = useState(input.symbolName.length === 0)
        const [symbolQueryInput, setSymbolQueryInput] = useState(input.initialQueryInput ?? '')
        const debouncedSetSymbolQueryInput = useMemo(() => debounce(setSymbolQueryInput, 300), [setSymbolQueryInput])

        const onSymbolSelected = useCallback(
            (input: SymbolBlockInput) => {
                onBlockInputChange(id, { type: 'symbol', input })
                onRunBlock(id)
            },
            [id, onBlockInputChange, onRunBlock]
        )

        const focusInput = useCallback(() => {
            if (editor) {
                focusEditor(editor)
            }
        }, [editor])

        const hideInputs = useCallback(() => setShowInputs(false), [setShowInputs])

        const symbolOutput = useObservable(useMemo(() => output?.pipe(startWith(LOADING)) ?? of(undefined), [output]))

        const commonMenuActions = useCommonBlockMenuActions({
            id,
            isReadOnly,
            ...props,
        })

        const symbolURL = useMemo(
            () =>
                isSymbolOutputLoaded(symbolOutput)
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
                    icon: <Icon aria-hidden={true} svgPath={mdiOpenInNew} />,
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
                    icon: <Icon aria-hidden={true} svgPath={showInputs ? mdiCheck : mdiPencil} />,
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

        const logEventOnCopy = useCallback(() => {
            telemetryService.log(...codeCopiedEvent('notebook-symbols'))
        }, [telemetryService])

        const viewerUpdates = useCodeIntelViewerUpdates(codeIntelViewerUpdatesProps)

        return (
            <NotebookBlock
                className={styles.block}
                id={id}
                aria-label="Notebook symbol block"
                isInputVisible={showInputs}
                setIsInputVisible={setShowInputs}
                focusInput={focusInput}
                isReadOnly={isReadOnly}
                isSelected={isSelected}
                isOtherBlockSelected={isOtherBlockSelected}
                actions={isSelected ? menuActions : linkMenuAction}
                {...props}
            >
                <div className={styles.header}>
                    {input.symbolName.length > 0 ? (
                        <NotebookSymbolBlockHeader
                            {...input}
                            symbolFoundAtLatestRevision={
                                isSymbolOutputLoaded(symbolOutput)
                                    ? symbolOutput.symbolFoundAtLatestRevision
                                    : undefined
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
                        queryInput={symbolQueryInput}
                        isLightTheme={isLightTheme}
                        onEditorCreated={setEditor}
                        setQueryInput={debouncedSetSymbolQueryInput}
                        onSymbolSelected={onSymbolSelected}
                        onRunBlock={hideInputs}
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
                            className={styles.code}
                            repoName={input.repositoryName}
                            commitID={input.revision}
                            filePath={input.filePath}
                            blobLines={symbolOutput.highlightedLines}
                            highlightRanges={[symbolOutput.highlightSymbolRange]}
                            {...symbolOutput.highlightLineRange}
                            fetchHighlightedFileRangeLines={() => of([])}
                            hoverifier={hoverifier}
                            viewerUpdates={viewerUpdates}
                            onCopy={logEventOnCopy}
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
)

interface NotebookSymbolBlockHeaderProps extends SymbolBlockInput {
    symbolFoundAtLatestRevision: boolean | undefined
    effectiveRevision: string
    symbolURL: string
}

const NotebookSymbolBlockHeader: React.FunctionComponent<React.PropsWithChildren<NotebookSymbolBlockHeaderProps>> = ({
    repositoryName,
    filePath,
    symbolFoundAtLatestRevision,
    effectiveRevision,
    symbolName,
    symbolKind,
    symbolURL,
}) => {
    const repoAtRevisionURL = getRepositoryUrl(repositoryName, [effectiveRevision])
    return (
        <>
            <SymbolIcon kind={symbolKind} />
            <div className={styles.separator} />
            <RepoFileSymbolLink
                repoName={repositoryName}
                repoURL={repoAtRevisionURL}
                filePath={filePath}
                fileURL={symbolURL}
                symbolURL={symbolURL}
                symbolName={symbolName}
            />
            {symbolFoundAtLatestRevision === false && (
                <Tooltip
                    content={`Symbol not found at the latest revision, showing symbol at revision ${effectiveRevision}.`}
                >
                    <Icon
                        aria-label={`Symbol not found at the latest revision, showing symbol at revision ${effectiveRevision}.`}
                        className="ml-1"
                        svgPath={mdiInformationOutline}
                    />
                </Tooltip>
            )}
        </>
    )
}
