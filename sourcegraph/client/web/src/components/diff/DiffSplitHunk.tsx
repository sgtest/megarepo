import * as React from 'react'

import classNames from 'classnames'
import { useLocation } from 'react-router'

import { isDefined, property } from '@sourcegraph/common'
import { TextDocumentDecoration } from '@sourcegraph/extension-api-types'
import { DecorationMapByLine, decorationStyleForTheme } from '@sourcegraph/shared/src/api/extension/api/decorations'
import { ThemeProps } from '@sourcegraph/shared/src/theme'

import { DiffHunkLineType, FileDiffHunkFields } from '../../graphql-operations'

import { addLineNumberToHunks } from './addLineNumberToHunks'
import { DiffBoundary } from './DiffBoundary'
import { EmptyLine, Line } from './Lines'

import diffHunkStyles from './DiffHunk.module.scss'
import linesStyles from './Lines.module.scss'

type HunkZipped = [Hunk[], Hunk | undefined, number]

const splitDiff = (hunks: Hunk[]): HunkZipped =>
    hunks.reduce(
        ([result, last, lastDeletionIndex], current, index): HunkZipped => {
            if (!last) {
                result.push(current)
                return [result, current, current.kind === DiffHunkLineType.DELETED ? index : -1]
            }

            if (current.kind === DiffHunkLineType.ADDED && lastDeletionIndex >= 0) {
                result.splice(lastDeletionIndex + 1, 0, current)
                return [result, current, lastDeletionIndex + 2]
            }

            result.push(current)

            // Preserve `lastDeletionIndex` if there are lines of deletions,
            // otherwise update it to the new deletion line
            let newLastDeletionIndex = -1
            if (current.kind === DiffHunkLineType.DELETED) {
                if (last.kind === DiffHunkLineType.DELETED) {
                    newLastDeletionIndex = lastDeletionIndex
                } else {
                    newLastDeletionIndex = index
                }
            }
            return [result, current, newLastDeletionIndex]
        },
        [[], undefined, -1] as HunkZipped
    )

export interface DiffHunkProps extends ThemeProps {
    /** The anchor (URL hash link) of the file diff. The component creates sub-anchors with this prefix. */
    fileDiffAnchor: string
    hunk: FileDiffHunkFields
    lineNumbers: boolean
    decorations: Record<'head' | 'base', DecorationMapByLine>
    /**
     * Reflect selected line in url
     *
     * @default true
     */
    persistLines?: boolean
}

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
const addDecorations = (isLightTheme: boolean, decorationsForLine: TextDocumentDecoration[]) => {
    const lineStyle = decorationsForLine
        .filter(decoration => decoration.isWholeLine)
        .map(decoration => decorationStyleForTheme(decoration, isLightTheme))
        .reduce((style, decoration) => ({ ...style, ...decoration }), {})

    const decorationsWithAfterProperty = decorationsForLine.filter(property('after', isDefined))

    return { lineStyle, decorationsWithAfterProperty }
}

export interface Hunk {
    kind: DiffHunkLineType
    html: string
    anchor: string
    oldLine?: number
    newLine?: number
}

export const DiffSplitHunk: React.FunctionComponent<React.PropsWithChildren<DiffHunkProps>> = ({
    fileDiffAnchor,
    decorations,
    hunk,
    lineNumbers,
    persistLines = true,
    isLightTheme,
}) => {
    const location = useLocation()

    const { hunksWithLineNumber } = addLineNumberToHunks(
        hunk.highlight.lines,
        hunk.newRange.startLine,
        hunk.oldRange.startLine,
        fileDiffAnchor
    )

    const [diff] = React.useMemo(() => splitDiff(hunksWithLineNumber), [hunksWithLineNumber])

    const groupHunks = React.useCallback(
        (hunks: Hunk[]): JSX.Element[] => {
            const elements = []
            for (let index = 0; index < hunks.length; index++) {
                const current = hunks[index]

                const lineNumber = (elements[index + 1] ? current.oldLine : current.newLine) as number
                const active = location.hash === `#${current.anchor}`

                const decorationsForLine = [
                    // If the line was deleted, look for decorations in the base revision
                    ...((current.kind === DiffHunkLineType.DELETED && decorations.base.get(lineNumber)) || []),
                    // If the line wasn't deleted, look for decorations in the head revision
                    ...((current.kind !== DiffHunkLineType.DELETED && decorations.head.get(lineNumber)) || []),
                ]

                const { lineStyle, decorationsWithAfterProperty } = addDecorations(isLightTheme, decorationsForLine)

                const rowProps = {
                    key: current.anchor,
                    'data-split-mode': 'split',
                    'data-testid': current.anchor,
                }

                const lineProps = {
                    persistLines,
                    lineStyle,
                    decorations: decorationsWithAfterProperty,
                    className: active ? linesStyles.lineActive : '',
                    lineNumbers,
                    html: current.html,
                    anchor: current.anchor,
                    kind: current.kind,
                    isLightTheme,
                }

                if (current.kind === DiffHunkLineType.UNCHANGED) {
                    // UNCHANGED is displayed on both side
                    elements.push(
                        <tr {...rowProps}>
                            <Line
                                {...lineProps}
                                key={`L${current.anchor}`}
                                id={`L${current.anchor}`}
                                lineNumber={current.oldLine}
                                dataPart="base"
                            />
                            <Line
                                {...lineProps}
                                key={`R${current.anchor}`}
                                id={`R${current.anchor}`}
                                lineNumber={current.newLine}
                                dataPart="head"
                            />
                        </tr>
                    )
                } else if (current.kind === DiffHunkLineType.DELETED) {
                    const next = hunks[index + 1]
                    // If an ADDED change is following a DELETED change, they should be displayed side by side
                    if (next?.kind === DiffHunkLineType.ADDED) {
                        index = index + 1
                        elements.push(
                            <tr {...rowProps}>
                                <Line
                                    {...lineProps}
                                    key={current.anchor}
                                    lineNumber={current.oldLine}
                                    dataPart="base"
                                />
                                <Line
                                    {...lineProps}
                                    key={next.anchor}
                                    kind={next.kind}
                                    lineNumber={next.newLine}
                                    anchor={next.anchor}
                                    html={next.html}
                                    className={classNames(
                                        location.hash === `#${next.anchor}` && linesStyles.lineActive
                                    )}
                                    dataPart="head"
                                />
                            </tr>
                        )
                    } else {
                        // DELETED is following by an empty line
                        elements.push(
                            <tr {...rowProps}>
                                <Line
                                    {...lineProps}
                                    key={current.anchor}
                                    lineNumber={
                                        current.kind === DiffHunkLineType.DELETED ? current.oldLine : lineNumber
                                    }
                                    dataPart="base"
                                />
                                <EmptyLine />
                            </tr>
                        )
                    }
                } else {
                    // ADDED is preceded by an empty line
                    elements.push(
                        <tr {...rowProps}>
                            <EmptyLine />
                            <Line {...lineProps} key={current.anchor} lineNumber={lineNumber} dataPart="head" />
                        </tr>
                    )
                }
            }

            return elements
        },
        [decorations.base, decorations.head, isLightTheme, location.hash, lineNumbers, persistLines]
    )

    const diffView = React.useMemo(() => groupHunks(diff), [diff, groupHunks])

    return (
        <>
            <DiffBoundary
                {...hunk}
                contentClassName={diffHunkStyles.content}
                lineNumbers={lineNumbers}
                diffMode="split"
            />
            {diffView}
        </>
    )
}
