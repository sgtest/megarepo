/**
 * Returns the sum of the number of matches of the patterns in the string.
 *
 * @param patterns Patterns to match in the string.
 */
export function count(str: string, ...patterns: RegExp[]): number {
    let n = 0
    for (const p of patterns) {
        if (!p.global) {
            throw new Error('expected RegExp to be global (or else count is inaccurate)')
        }
        const m = str.match(p)
        if (m) {
            n += m.length
        }
    }
    return n
}

export function numberWithCommas(x: string | number): string {
    return x.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',')
}

export function pluralize(str: string, n: number, plural = str + 's'): string {
    return n === 1 ? str : plural
}

/**
 * Replaces all non alphabetic characters with `-` and lowercases the result.
 */
export function sanitizeClass(value: string): string {
    return value.replace(/[^A-Za-z]/g, '-').toLowerCase()
}

/**
 * In the given string, deduplicate whitespace.
 * E.g: " a  b  c  " => " a b c "
 */
export function dedupeWhitespace(value: string): string {
    return value.replace(/\s+/g, ' ')
}

/**
 * Checkes whether a given string is quoted.
 *
 * @param value string to check against
 */
export function isQuoted(value: string): boolean {
    return value.startsWith('"') && value.endsWith('"') && value !== '"'
}

/**
 * Replaces a substring within a string.
 *
 * @param s Original string
 * @param range The range in of the substring to be replaced
 * @param replacement an optional replacement string
 */
export function replaceRange(s: string, { start, end }: { start: number; end: number }, replacement = ''): string {
    return s.slice(0, start) + replacement + s.slice(end)
}
