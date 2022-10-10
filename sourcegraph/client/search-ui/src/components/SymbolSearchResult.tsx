import React, { useCallback, KeyboardEvent, MouseEvent } from 'react'

import classNames from 'classnames'
import { useHistory } from 'react-router'
import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { isErrorLike } from '@sourcegraph/common'
import { HighlightLineRange, HighlightResponseFormat } from '@sourcegraph/search'
import { getFileMatchUrl, getRepositoryUrl, getRevision, SymbolMatch } from '@sourcegraph/shared/src/search/stream'
import { SettingsCascadeProps } from '@sourcegraph/shared/src/settings/settings'
import { SymbolTag } from '@sourcegraph/shared/src/symbols/SymbolTag'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { codeCopiedEvent } from '@sourcegraph/shared/src/tracking/event-log-creators'

import { CodeExcerpt, FetchFileParameters } from './CodeExcerpt'
import { navigateToCodeExcerpt, navigateToFileOnMiddleMouseButtonClick } from './codeLinkNavigation'
import { CopyPathAction } from './CopyPathAction'
import { LastSyncedIcon } from './LastSyncedIcon'
import { RepoFileLink } from './RepoFileLink'
import { ResultContainer } from './ResultContainer'

import searchResultStyles from './SearchResult.module.scss'
import styles from './SymbolSearchResult.module.scss'

export interface SymbolSearchResultProps extends TelemetryProps, SettingsCascadeProps {
    result: SymbolMatch
    openInNewTab?: boolean
    repoDisplayName: string
    containerClassName?: string
    index: number
    onSelect: () => void
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

export const SymbolSearchResult: React.FunctionComponent<SymbolSearchResultProps> = ({
    result,
    openInNewTab,
    repoDisplayName,
    onSelect,
    containerClassName,
    index,
    telemetryService,
    settingsCascade,
    fetchHighlightedFileLineRanges,
}) => {
    const enableLazyFileResultSyntaxHighlighting =
        settingsCascade.final &&
        !isErrorLike(settingsCascade.final) &&
        settingsCascade.final.experimentalFeatures?.enableLazyFileResultSyntaxHighlighting

    const repoAtRevisionURL = getRepositoryUrl(result.repository, result.branches)
    const revisionDisplayName = getRevision(result.branches, result.commit)

    const title = (
        <span>
            <RepoFileLink
                repoName={result.repository}
                repoURL={repoAtRevisionURL}
                filePath={result.path}
                fileURL={getFileMatchUrl(result)}
                repoDisplayName={
                    repoDisplayName
                        ? `${repoDisplayName}${revisionDisplayName ? `@${revisionDisplayName}` : ''}`
                        : undefined
                }
                className={classNames(searchResultStyles.titleInner, searchResultStyles.mutedRepoFileLink)}
            />
            <CopyPathAction
                filePath={result.path}
                className={searchResultStyles.copyButton}
                telemetryService={telemetryService}
            />
        </span>
    )

    const history = useHistory()
    const navigateToFile = useCallback(
        (event: KeyboardEvent<HTMLElement> | MouseEvent<HTMLElement>): void =>
            navigateToCodeExcerpt(event, openInNewTab ?? false, history),
        [openInNewTab, history]
    )

    const logEventOnCopy = useCallback(() => {
        telemetryService.log(...codeCopiedEvent('file-match'))
    }, [telemetryService])

    const fetchSymbolMatchLineRanges = useCallback(
        (startLine: number, endLine: number, format: HighlightResponseFormat) => {
            const startTime = Date.now()
            return fetchHighlightedFileLineRanges({
                repoName: result.repository,
                commitID: result.commit || '',
                filePath: result.path,
                disableTimeout: false,
                format,
                ranges: result.symbols.map(
                    (symbol): HighlightLineRange => ({
                        startLine: symbol.line - 1,
                        endLine: symbol.line,
                    })
                ),
            }).pipe(
                map(lines => {
                    const endTime = Date.now()
                    telemetryService.log(
                        'search.latencies.frontend.code-load',
                        { durationMs: endTime - startTime },
                        { durationMs: endTime - startTime }
                    )
                    return lines[
                        result.symbols.findIndex(symbol => symbol.line - 1 === startLine && symbol.line === endLine)
                    ]
                })
            )
        },
        [result, fetchHighlightedFileLineRanges, telemetryService]
    )

    const fetchHighlightedSymbolMatchLineRanges = useCallback(
        (startLine: number, endLine: number) =>
            fetchSymbolMatchLineRanges(startLine, endLine, HighlightResponseFormat.HTML_HIGHLIGHT),
        [fetchSymbolMatchLineRanges]
    )

    const fetchPlainTextSymbolMatchLineRanges = useCallback(
        (startLine: number, endLine: number) =>
            fetchSymbolMatchLineRanges(startLine, endLine, HighlightResponseFormat.HTML_PLAINTEXT),
        [fetchSymbolMatchLineRanges]
    )

    return (
        <ResultContainer
            index={index}
            title={title}
            resultType={result.type}
            onResultClicked={onSelect}
            repoName={result.repository}
            repoStars={result.repoStars}
            className={classNames(searchResultStyles.copyButtonContainer, containerClassName)}
            resultClassName={styles.symbolsOverride}
        >
            <div className={styles.symbols}>
                {result.repoLastFetched && <LastSyncedIcon lastSyncedTime={result.repoLastFetched} />}
                {result.symbols.map(symbol => (
                    <div
                        key={`symbol:${symbol.name}${String(symbol.containerName)}${symbol.url}`}
                        className={styles.symbol}
                        data-href={symbol.url}
                        role="link"
                        data-testid="symbol-search-result"
                        tabIndex={0}
                        onClick={navigateToFile}
                        onMouseUp={navigateToFileOnMiddleMouseButtonClick}
                        onKeyDown={navigateToFile}
                    >
                        <div className="mr-2 flex-shrink-0">
                            <SymbolTag kind={symbol.kind} />
                        </div>
                        <div className={styles.symbolCodeExcerpt}>
                            <CodeExcerpt
                                className="a11y-ignore"
                                repoName={result.repository}
                                commitID={result.commit || ''}
                                filePath={result.path}
                                startLine={symbol.line - 1}
                                endLine={symbol.line}
                                fetchHighlightedFileRangeLines={fetchHighlightedSymbolMatchLineRanges}
                                fetchPlainTextFileRangeLines={
                                    enableLazyFileResultSyntaxHighlighting
                                        ? fetchPlainTextSymbolMatchLineRanges
                                        : undefined
                                }
                                onCopy={logEventOnCopy}
                                highlightRanges={[]}
                            />
                        </div>
                    </div>
                ))}
            </div>
        </ResultContainer>
    )
}
