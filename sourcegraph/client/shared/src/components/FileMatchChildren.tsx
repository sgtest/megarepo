import classNames from 'classnames'
import * as H from 'history'
import React, { MouseEvent, KeyboardEvent, useCallback } from 'react'
import { useHistory } from 'react-router'
import { Observable } from 'rxjs'
import { map } from 'rxjs/operators'

import { isErrorLike } from '@sourcegraph/common'
import { Link } from '@sourcegraph/wildcard'

import { IHighlightLineRange } from '../schema'
import { ContentMatch, SymbolMatch, PathMatch, getFileMatchUrl } from '../search/stream'
import { SettingsCascadeProps } from '../settings/settings'
import { SymbolIcon } from '../symbols/SymbolIcon'
import { TelemetryProps } from '../telemetry/telemetryService'
import {
    appendLineRangeQueryParameter,
    toPositionOrRangeQueryParameter,
    appendSubtreeQueryParameter,
} from '../util/url'

import { CodeExcerpt, FetchFileParameters } from './CodeExcerpt'
import styles from './FileMatchChildren.module.scss'
import { LastSyncedIcon } from './LastSyncedIcon'
import { MatchGroup } from './ranking/PerFileResultRanking'

interface FileMatchProps extends SettingsCascadeProps, TelemetryProps {
    location: H.Location
    result: ContentMatch | SymbolMatch | PathMatch
    grouped: MatchGroup[]
    /* Called when the first result has fully loaded. */
    onFirstResultLoad?: () => void
    fetchHighlightedFileLineRanges: (parameters: FetchFileParameters, force?: boolean) => Observable<string[][]>
}

/**
 * This helper function determines whether a mouse/click event was triggered as
 * a result of selecting text in search results.
 * There are at least to ways to do this:
 *
 * - Tracking `mouseup`, `mousemove` and `mousedown` events. The occurrence of
 * a `mousemove` event would indicate a text selection. However, users
 * might slightly move the mouse while clicking, and solutions that would
 * take this into account seem fragile.
 * - (implemented here) Inspect the Selection object returned by
 * `window.getSelection()`.
 *
 * CAVEAT: Chromium and Firefox (and maybe other browsers) behave
 * differently when a search result is clicked *after* text selection was
 * made:
 *
 * - Firefox will clear the selection before executing the click event
 * handler, i.e. the search result will be opened.
 * - Chrome will only clear the selection if the click happens *outside*
 * of the selected text (in which case the search result will be
 * opened). If the click happens inside the selected text the selection
 * will be cleared only *after* executing the click event handler.
 */
function isTextSelectionEvent(event: MouseEvent<HTMLElement>): boolean {
    const selection = window.getSelection()

    // Text selections are always ranges. Should the type not be set, verify
    // that the selection is not empty.
    if (selection && (selection.type === 'Range' || selection.toString() !== '')) {
        // Firefox specific: Because our code excerpts are implemented as tables,
        // CTRL+click would select the table cell. Since users don't know that we
        // use tables, the most likely wanted to open the search results in a new
        // tab instead though.
        if ((event.ctrlKey || event.metaKey) && selection.anchorNode?.nodeName === 'TR') {
            // Ugly side effect: We don't want the table cell to be highlighted.
            // The focus style that Firefox uses doesn't seem to be affected by
            // CSS so instead we clear the selection.
            selection.empty()
            return false
        }

        return true
    }

    return false
}

export const FileMatchChildren: React.FunctionComponent<FileMatchProps> = props => {
    // If optimizeHighlighting is enabled, compile a list of the highlighted file ranges we want to
    // fetch (instead of the entire file.)
    const optimizeHighlighting =
        props.settingsCascade.final &&
        !isErrorLike(props.settingsCascade.final) &&
        props.settingsCascade.final.experimentalFeatures &&
        props.settingsCascade.final.experimentalFeatures.enableFastResultLoading

    const { result, grouped, fetchHighlightedFileLineRanges, telemetryService, onFirstResultLoad } = props
    const history = useHistory()
    const fetchHighlightedFileRangeLines = React.useCallback(
        (isFirst, startLine, endLine) => {
            const startTime = Date.now()
            return fetchHighlightedFileLineRanges(
                {
                    repoName: result.repository,
                    commitID: result.commit || '',
                    filePath: result.path,
                    disableTimeout: false,
                    ranges: optimizeHighlighting
                        ? grouped.map(
                              (group): IHighlightLineRange => ({
                                  startLine: group.startLine,
                                  endLine: group.endLine,
                              })
                          )
                        : [{ startLine: 0, endLine: 2147483647 }], // entire file,
                },
                false
            ).pipe(
                map(lines => {
                    if (isFirst && onFirstResultLoad) {
                        onFirstResultLoad()
                    }
                    telemetryService.log(
                        'search.latencies.frontend.code-load',
                        { durationMs: Date.now() - startTime },
                        { durationMs: Date.now() - startTime }
                    )
                    return optimizeHighlighting
                        ? lines[grouped.findIndex(group => group.startLine === startLine && group.endLine === endLine)]
                        : lines[0].slice(startLine, endLine)
                })
            )
        },
        [result, fetchHighlightedFileLineRanges, grouped, optimizeHighlighting, telemetryService, onFirstResultLoad]
    )

    const createCodeExcerptLink = (group: MatchGroup): string => {
        const positionOrRangeQueryParameter = toPositionOrRangeQueryParameter({ position: group.position })
        return appendLineRangeQueryParameter(
            appendSubtreeQueryParameter(getFileMatchUrl(result)),
            positionOrRangeQueryParameter
        )
    }

    /**
     * This handler implements the logic to simulate the click/keyboard
     * activation behavior of links, while also allowing the selection of text
     * inside the element.
     * Because a click event is dispatched in both cases (clicking the search
     * result to open it as well as selecting text within it), we have to be
     * able to distinguish between those two actions.
     * If we detect a text selection action, we don't have to do anything.
     *
     * CAVEATS:
     * - In Firefox, Shift+click will open the URL in a new tab instead of
     * a window (unlike Chromium which seems to show the same behavior as with
     * native links).
     * - Firefox will insert \t\n in between table rows, causing the copied
     * text to be different from what is in the file/search result.
     */
    const navigateToFile = useCallback(
        (event: KeyboardEvent<HTMLElement> | MouseEvent<HTMLElement>): void => {
            // Testing for text selection is only necessary for mouse/click
            // events.
            if (
                (event.type === 'click' && !isTextSelectionEvent(event as MouseEvent<HTMLElement>)) ||
                (event as KeyboardEvent<HTMLElement>).key === 'Enter'
            ) {
                const href = event.currentTarget.getAttribute('data-href')

                if (!event.defaultPrevented && href) {
                    event.preventDefault()
                    if (event.ctrlKey || event.metaKey || event.shiftKey) {
                        window.open(href, '_blank')
                    } else {
                        history.push(href)
                    }
                }
            }
        },
        [history]
    )

    return (
        <div className={styles.fileMatchChildren} data-testid="file-match-children">
            {result.repoLastFetched && <LastSyncedIcon lastSyncedTime={result.repoLastFetched} />}
            {/* Path */}
            {result.type === 'path' && (
                <div className={styles.item} data-testid="file-match-children-item">
                    <small>Path match</small>
                </div>
            )}

            {/* Symbols */}
            {((result.type === 'symbol' && result.symbols) || []).map(symbol => (
                <Link
                    to={symbol.url}
                    className={classNames('test-file-match-children-item', styles.item)}
                    key={`symbol:${symbol.name}${String(symbol.containerName)}${symbol.url}`}
                    data-testid="file-match-children-item"
                >
                    <SymbolIcon kind={symbol.kind} className="icon-inline mr-1" />
                    <code>
                        {symbol.name}{' '}
                        {symbol.containerName && <span className="text-muted">{symbol.containerName}</span>}
                    </code>
                </Link>
            ))}

            {/* Line matches */}
            {grouped && (
                <div>
                    {grouped.map((group, index) => (
                        <div
                            key={`linematch:${getFileMatchUrl(result)}${group.position.line}:${
                                group.position.character
                            }`}
                            className={classNames('test-file-match-children-item-wrapper', styles.itemCodeWrapper)}
                        >
                            <div
                                data-href={createCodeExcerptLink(group)}
                                className={classNames(
                                    'test-file-match-children-item',
                                    styles.item,
                                    styles.itemClickable
                                )}
                                onClick={navigateToFile}
                                onKeyDown={navigateToFile}
                                data-testid="file-match-children-item"
                                tabIndex={0}
                                role="link"
                            >
                                <CodeExcerpt
                                    repoName={result.repository}
                                    commitID={result.commit || ''}
                                    filePath={result.path}
                                    startLine={group.startLine}
                                    endLine={group.endLine}
                                    highlightRanges={group.matches}
                                    fetchHighlightedFileRangeLines={fetchHighlightedFileRangeLines}
                                    isFirst={index === 0}
                                    blobLines={group.blobLines}
                                />
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    )
}
