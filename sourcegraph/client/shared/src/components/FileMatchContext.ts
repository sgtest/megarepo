import { BadgeAttachmentRenderOptions } from 'sourcegraph'
import { flatMap } from 'lodash'
import { MatchItem } from './FileMatch'

/**
 * Groups highlights that have overlapping or adjacent context. The input must be sorted.
 */
export const mergeContext = <T extends { line: number }>(context: number, highlights: T[]): T[][] => {
    const groupsOfHighlights: T[][] = []

    for (let index = 0; index < highlights.length; index++) {
        const current = highlights[index]
        const previous = highlights[index - 1]
        if (!previous || current.line - previous.line - 2 * context > 1) {
            // Either this is the beginning of the file, or there is at
            // least one line between the end of the previous context
            // and the beginning of this context, so start a new group.
            groupsOfHighlights.push([current])
        } else {
            // This context either overlaps with or is adjacent to the
            // previous context, so add this highlight to the previous
            // group.
            groupsOfHighlights[groupsOfHighlights.length - 1].push(current)
        }
    }

    return groupsOfHighlights
}

const calculateGroupPositions = (
    matches: {
        line: number
        character: number
        highlightLength: number
        badge: BadgeAttachmentRenderOptions | undefined
        IsInContext: boolean
    }[],
    context: number,
    highestLineNumberWithinSubsetMatches: number
): MatchGroup => {
    {
        let startLine = matches[0].line - context
        startLine = startLine < 0 ? 0 : startLine

        const highlightRangeLines = matches.map(range => range.line)

        // The highest line number of all highlights in this excerpt.
        const lastHighlightLineNumber = Math.max(...highlightRangeLines)

        // If the last highlight line is greater than the highest line number within the subset of matches
        // we are displaying, then we know that there's at least one highlight in the context lines.
        const contextLineHasHighlight = lastHighlightLineNumber > highestLineNumberWithinSubsetMatches

        // The gap between the last highlight provided to this excerpt, and the line number of the last highlighted
        // match that is not a context line. If this value is larger than context lines, it means that we are
        // displaying _all_ matches, and therefore, do not need to reduce the number of context lines shown.
        const remainingContextLinesToShow = lastHighlightLineNumber - highestLineNumberWithinSubsetMatches

        const numberOfContextLinesToShow = contextLineHasHighlight
            ? context - (remainingContextLinesToShow <= context ? remainingContextLinesToShow : 0)
            : context

        // Of the matches in this excerpt, pick the one with the highest line number + lines of context.
        // Don't add the context value to calculate the last line if the last highlight match is the highlight range + context
        const endLine = contextLineHasHighlight
            ? Math.max(...highlightRangeLines) + numberOfContextLinesToShow
            : Math.max(...highlightRangeLines) + context

        return {
            matches,

            // 1-based position describing the starting place of the matches.
            position: { line: matches[0].line + 1, character: matches[0].character + 1 },

            // 0-based range describing the start and end lines (end line is exclusive.)
            startLine,
            endLine: endLine + 1,
        }
    }
}

/**
 * Describes a single group of matches.
 */
interface MatchGroup {
    // The matches in this group to display.
    matches: {
        line: number
        character: number
        highlightLength: number
        badge: BadgeAttachmentRenderOptions | undefined
        IsInContext: boolean
    }[]

    // The 1-based position of where the first match in the group.
    position: {
        line: number
        character: number
    }

    // The 0-based start line of the group (inclusive.)
    startLine: number

    // The 0-based end line of the group (exclusive.)
    endLine: number
}

/**
 * Calculates how to group together matches for display. Takes into account:
 *
 * - Whether or not displaying a subset or all matches is desired
 * - A surrounding number of context lines to display
 * - Whether or not context lines have (or do not have) matches within them
 * - Grouping based on whether or not there is overlapping or adjacent context.
 *
 * @param matches The matches to split into groups.
 * @param maxMatches The maximum number of matches to show, or 0 for all.
 * @param context The number of surrounding context lines to show for each match.
 * @returns The subset of matches that were sorted and chosen for display, as well as that same
 * list of matches grouped together.
 */
export const calculateMatchGroups = (
    matches: MatchItem[],
    maxMatches: number,
    context: number
): [MatchItem[], MatchGroup[]] => {
    const sortedMatches = matches.sort((a, b) => {
        if (a.line < b.line) {
            return -1
        }
        if (a.line === b.line) {
            if (a.highlightRanges[0].start < b.highlightRanges[0].start) {
                return -1
            }
            if (a.highlightRanges[0].start === b.highlightRanges[0].start) {
                return 0
            }
        }
        return 1
    })

    // This checks the highest line number amongst the number of matches
    // that we want to show in a collapsed result preview.
    const highestLineNumberWithinSubsetMatches =
        sortedMatches.length > 0
            ? sortedMatches.length > maxMatches
                ? sortedMatches[maxMatches === 0 ? 0 : maxMatches - 1].line
                : sortedMatches[sortedMatches.length - 1].line
            : 0

    // Determine which line matches we will show. This includes matches that are in the context
    // area (if any.)
    const showMatches = sortedMatches.filter(
        (match, index) =>
            maxMatches === 0 || index < maxMatches || match.line <= highestLineNumberWithinSubsetMatches + context
    )

    const grouped = mergeContext(
        context,
        flatMap(showMatches, (match, index) =>
            match.highlightRanges.map(range => ({
                line: match.line,
                character: range.start,
                highlightLength: range.highlightLength,
                badge: match.badge,
                IsInContext: maxMatches === 0 ? false : match.line > highestLineNumberWithinSubsetMatches,
            }))
        )
    ).map(group => calculateGroupPositions(group, context, highestLineNumberWithinSubsetMatches))

    return [showMatches, grouped]
}
