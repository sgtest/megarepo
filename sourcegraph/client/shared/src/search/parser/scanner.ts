import { IRange } from 'monaco-editor'
import { filterTypeKeysWithAliases } from '../interactive/util'

/**
 * Represents a zero-indexed character range in a single-line search query.
 */
export interface CharacterRange {
    /** Zero-based character on the line */
    start: number
    /** Zero-based character on the line */
    end: number
}

/**
 * Converts a zero-indexed, single-line {@link CharacterRange} to a Monaco {@link IRange}.
 */
export const toMonacoRange = ({ start, end }: CharacterRange): IRange => ({
    startLineNumber: 1,
    endLineNumber: 1,
    startColumn: start + 1,
    endColumn: end + 1,
})

/**
 * A label associated with a pattern token. We don't use SearchPatternType because
 * that is used as a global quantifier for all patterns in a query. PatternKind
 * allows to qualify multiple pattern tokens differently within a single query.
 */
export enum PatternKind {
    Literal = 1,
    Regexp,
    Structural,
}

export interface Pattern {
    type: 'pattern'
    range: CharacterRange
    kind: PatternKind
    value: string
}

/**
 * Represents a literal in a search query.
 *
 * Example: `Conn`.
 */
export interface Literal {
    type: 'literal'
    range: CharacterRange
    value: string
}

/**
 * Represents a filter in a search query.
 *
 * Example: `repo:^github\.com\/sourcegraph\/sourcegraph$`.
 */
export interface Filter {
    type: 'filter'
    range: CharacterRange
    filterType: Literal
    filterValue: Quoted | Literal | undefined
    negated: boolean
}

enum OperatorKind {
    Or = 'or',
    And = 'and',
    Not = 'not',
}

/**
 * Represents an operator in a search query.
 *
 * Example: AND, OR, NOT.
 */
export interface Operator {
    type: 'operator'
    value: string
    range: CharacterRange
    kind: OperatorKind
}

/**
 * Represents a sequence of tokens in a search query.
 */
export interface Sequence {
    type: 'sequence'
    range: CharacterRange
    members: Token[]
}

/**
 * Represents a quoted string in a search query.
 *
 * Example: "Conn".
 */
export interface Quoted {
    type: 'quoted'
    range: CharacterRange
    quotedValue: string
}

/**
 * Represents a C-style comment, terminated by a newline.
 *
 * Example: `// Oh hai`
 */
export interface Comment {
    type: 'comment'
    range: CharacterRange
    value: string
}

export interface Whitespace {
    type: 'whitespace'
    range: CharacterRange
}

export interface OpeningParen {
    type: 'openingParen'
    range: CharacterRange
}

export interface ClosingParen {
    type: 'closingParen'
    range: CharacterRange
}

export type Token = Whitespace | OpeningParen | ClosingParen | Operator | Comment | Literal | Pattern | Filter | Quoted

export type Term = Token | Sequence

/**
 * Represents the failed result of running a {@link Scanner} on a search query.
 */
interface ScanError {
    type: 'error'

    /**
     * A string representing the token that would have been expected
     * for successful scanning at {@link ScannerError#at}.
     */
    expected: string

    /**
     * The index in the search query string where parsing failed.
     */
    at: number
}

/**
 * Represents the successful result of running a {@link Scannerer} on a search query.
 */
export interface ScanSuccess<T = Term> {
    type: 'success'

    /**
     * The resulting term.
     */
    token: T
}

/**
 * Represents the result of running a {@link Scanner} on a search query.
 */
export type ScanResult<T = Term> = ScanError | ScanSuccess<T>

type Scanner<T = Term> = (input: string, start: number) => ScanResult<T>

/**
 * Returns a {@link Scanner} that succeeds if zero or more tokens are scanned
 * by the given `scanToken` scanners.
 */
const zeroOrMore = (scanToken: Scanner<Term>): Scanner<Sequence> => (input, start) => {
    const members: Token[] = []
    let adjustedStart = start
    let end = start + 1
    while (input[adjustedStart] !== undefined) {
        const result = scanToken(input, adjustedStart)
        if (result.type === 'error') {
            return result
        }
        if (result.token.type === 'sequence') {
            for (const member of result.token.members) {
                members.push(member)
            }
        } else {
            members.push(result.token)
        }
        end = result.token.range.end
        adjustedStart = end
    }
    return {
        type: 'success',
        token: { type: 'sequence', members, range: { start, end } },
    }
}

/**
 * Returns a {@link Scanner} that succeeds if any of the given scanner succeeds.
 */
const oneOf = <T>(...scanners: Scanner<T>[]): Scanner<T> => (input, start) => {
    const expected: string[] = []
    for (const scanner of scanners) {
        const result = scanner(input, start)
        if (result.type === 'success') {
            return result
        }
        expected.push(result.expected)
    }
    return {
        type: 'error',
        expected: `One of: ${expected.join(', ')}`,
        at: start,
    }
}

/**
 * A {@link Scanner} that will attempt to scan delimited strings for an arbitrary
 * delimiter. `\` is treated as an escape character for the delimited string.
 */
const quoted = (delimiter: string): Scanner<Quoted> => (input, start) => {
    if (input[start] !== delimiter) {
        return { type: 'error', expected: delimiter, at: start }
    }
    let end = start + 1
    while (input[end] && (input[end] !== delimiter || input[end - 1] === '\\')) {
        end = end + 1
    }
    if (!input[end]) {
        return { type: 'error', expected: delimiter, at: end }
    }
    return {
        type: 'success',
        // end + 1 as `end` is currently the index of the quote in the string.
        token: { type: 'quoted', quotedValue: input.slice(start + 1, end), range: { start, end: end + 1 } },
    }
}

/**
 * Returns a {@link Scanner} that will attempt to scan tokens matching
 * the given character in a search query.
 */
const character = (character: string): Scanner<Literal> => (input, start) => {
    if (input[start] !== character) {
        return { type: 'error', expected: character, at: start }
    }
    return {
        type: 'success',
        token: { type: 'literal', value: character, range: { start, end: start + 1 } },
    }
}

/**
 * Returns a {@link Scanner} that will attempt to scan
 * tokens matching the given RegExp pattern in a search query.
 */
const scanToken = <T extends Term = Literal>(
    regexp: RegExp,
    output?: T | ((input: string, range: CharacterRange) => T),
    expected?: string
): Scanner<T> => {
    if (!regexp.source.startsWith('^')) {
        regexp = new RegExp(`^${regexp.source}`, regexp.flags)
    }
    return (input, start) => {
        const matchTarget = input.slice(Math.max(0, start))
        if (!matchTarget) {
            return { type: 'error', expected: expected || `/${regexp.source}/`, at: start }
        }
        const match = matchTarget.match(regexp)
        if (!match) {
            return { type: 'error', expected: expected || `/${regexp.source}/`, at: start }
        }
        const range = { start, end: start + match[0].length }
        return {
            type: 'success',
            token: output
                ? typeof output === 'function'
                    ? output(input, range)
                    : output
                : ({ type: 'literal', value: match[0], range } as T),
        }
    }
}

const whitespace = scanToken(/\s+/, (_input, range) => ({
    type: 'whitespace',
    range,
}))

const literal = scanToken(/[^\s)]+/)

const operatorNot = scanToken(/(not|NOT)/, (input, { start, end }) => ({
    type: 'operator',
    value: input.slice(start, end),
    range: { start, end },
    kind: OperatorKind.Not,
}))

const operatorAnd = scanToken(/(and|AND)/, (input, { start, end }) => ({
    type: 'operator',
    value: input.slice(start, end),
    range: { start, end },
    kind: OperatorKind.And,
}))

const operatorOr = scanToken(/(or|OR)/, (input, { start, end }) => ({
    type: 'operator',
    value: input.slice(start, end),
    range: { start, end },
    kind: OperatorKind.Or,
}))

const operator = oneOf<Operator>(operatorAnd, operatorOr, operatorNot)

const comment = scanToken(
    /\/\/.*/,
    (input, { start, end }): Comment => ({ type: 'comment', value: input.slice(start, end), range: { start, end } })
)

const filterKeyword = scanToken(new RegExp(`-?(${filterTypeKeysWithAliases.join('|')})+(?=:)`, 'i'))

const filterDelimiter = character(':')

const filterValue = oneOf<Quoted | Literal>(quoted('"'), quoted("'"), literal)

const openingParen = scanToken(/\(/, (_input, range): OpeningParen => ({ type: 'openingParen', range }))

const closingParen = scanToken(/\)/, (_input, range): ClosingParen => ({ type: 'closingParen', range }))

/**
 * Returns a {@link Scanner} that succeeds if a token scanned by `scanToken`,
 * followed by whitespace or EOF, is found in the search query.
 */
const followedBy = (scanToken: Scanner<Token>, scanNext: Scanner<Token>): Scanner<Sequence> => (input, start) => {
    const members: Token[] = []
    const tokenResult = scanToken(input, start)
    if (tokenResult.type === 'error') {
        return tokenResult
    }
    members.push(tokenResult.token)
    let { end } = tokenResult.token.range
    if (input[end] !== undefined) {
        const separatorResult = scanNext(input, end)
        if (separatorResult.type === 'error') {
            return separatorResult
        }
        members.push(separatorResult.token)
        end = separatorResult.token.range.end
    }
    return {
        type: 'success',
        token: { type: 'sequence', members, range: { start, end } },
    }
}

/**
 * A {@link Scanner} that will attempt to scan {@link Filter} tokens
 * (consisting a of a filter type and a filter value, separated by a colon)
 * in a search query.
 */
const filter: Scanner<Filter> = (input, start) => {
    const scannedKeyword = filterKeyword(input, start)
    if (scannedKeyword.type === 'error') {
        return scannedKeyword
    }
    const scannedDelimiter = filterDelimiter(input, scannedKeyword.token.range.end)
    if (scannedDelimiter.type === 'error') {
        return scannedDelimiter
    }
    const scannedValue =
        input[scannedDelimiter.token.range.end] === undefined
            ? undefined
            : filterValue(input, scannedDelimiter.token.range.end)
    if (scannedValue && scannedValue.type === 'error') {
        return scannedValue
    }
    return {
        type: 'success',
        token: {
            type: 'filter',
            range: { start, end: scannedValue ? scannedValue.token.range.end : scannedDelimiter.token.range.end },
            filterType: scannedKeyword.token,
            filterValue: scannedValue?.token,
            negated: scannedKeyword.token.value.startsWith('-'),
        },
    }
}

const createPattern = (value: string, range: CharacterRange, kind: PatternKind): ScanSuccess<Pattern> => ({
    type: 'success',
    token: {
        type: 'pattern',
        range,
        kind,
        value,
    },
})

const scanFilterOrOperator = oneOf<Literal | Sequence>(filterKeyword, followedBy(operator, whitespace))
const keepScanning = (input: string, start: number): boolean => scanFilterOrOperator(input, start).type !== 'success'

/**
 * ScanBalancedPattern attempts to scan balanced parentheses as literal patterns. This
 * ensures that we interpret patterns containing parentheses _as patterns_ and not
 * groups. For example, it accepts these patterns:
 *
 * ((a|b)|c)              - a regular expression with balanced parentheses for grouping
 * myFunction(arg1, arg2) - a literal string with parens that should be literally interpreted
 * foo(...)               - a structural search pattern
 *
 * If it weren't for this scanner, the above parentheses would have to be
 * interpreted as part of the query language group syntax, like these:
 *
 * (foo or (bar and baz))
 *
 * So, this scanner detects parentheses as patterns without needing the user to
 * explicitly escape them. As such, there are cases where this scanner should
 * not succeed:
 *
 * (foo or (bar and baz)) - a valid query with and/or expression groups in the query langugae
 * (repo:foo bar baz)     - a valid query containing a recognized repo: field. Here parentheses are interpreted as a group, not a pattern.
 */
export const scanBalancedPattern = (kind = PatternKind.Literal): Scanner<Pattern> => (input, start) => {
    let adjustedStart = start
    let balanced = 0
    let current = ''
    const result: string[] = []

    const nextChar = (): void => {
        current = input[adjustedStart]
        adjustedStart += 1
    }

    if (!keepScanning(input, start)) {
        return {
            type: 'error',
            expected: 'no recognized filter or operator',
            at: start,
        }
    }

    while (input[adjustedStart] !== undefined) {
        nextChar()
        if (current === ' ' && balanced === 0) {
            // Stop scanning a potential pattern when we see whitespace in a balanced state.
            adjustedStart -= 1 // Backtrack.
            break
        } else if (current === '(') {
            if (!keepScanning(input, adjustedStart)) {
                return {
                    type: 'error',
                    expected: 'no recognized filter or operator',
                    at: adjustedStart,
                }
            }
            balanced += 1
            result.push(current)
        } else if (current === ')') {
            balanced -= 1
            if (balanced < 0) {
                // This paren is an unmatched closing paren, so we stop treating it as a potential
                // pattern here--it might be closing a group.
                adjustedStart -= 1 // Backtrack.
                balanced = 0 // Pattern is balanced up to this point
                break
            }
            result.push(current)
        } else if (current === ' ') {
            if (!keepScanning(input, adjustedStart)) {
                return {
                    type: 'error',
                    expected: 'no recognized filter or operator',
                    at: adjustedStart,
                }
            }
            result.push(current)
        } else if (current === '\\') {
            if (input[adjustedStart] !== undefined) {
                nextChar()
                // Accept anything anything escaped. The point is to consume escaped spaces like "\ "
                // so that we don't recognize it as terminating a pattern.
                result.push('\\', current)
                continue
            }
            result.push(current)
        } else {
            result.push(current)
        }
    }

    if (balanced !== 0) {
        return {
            type: 'error',
            expected: 'no unbalanced parentheses',
            at: adjustedStart,
        }
    }

    return createPattern(result.join(''), { start, end: adjustedStart }, kind)
}

const scanPattern = (kind: PatternKind): Scanner<Pattern> => (input, start) => {
    const balancedPattern = scanBalancedPattern(kind)(input, start)
    if (balancedPattern.type === 'success') {
        return createPattern(balancedPattern.token.value, balancedPattern.token.range, kind)
    }

    const anyPattern = literal(input, start)
    if (anyPattern.type === 'success') {
        return createPattern(anyPattern.token.value, anyPattern.token.range, kind)
    }

    return anyPattern
}

const whitespaceOrClosingParen = oneOf<Whitespace | ClosingParen>(whitespace, closingParen)

/**
 * A {@link Scanner} for a Sourcegraph search query, interpreting patterns for {@link PatternKind}.
 *
 * @param interpretComments Interpets C-style line comments for multiline queries.
 */
const createScanner = (kind: PatternKind, interpretComments?: boolean): Scanner<Sequence> => {
    const baseQuotedScanner = [quoted('"'), quoted("'")]
    const quotedScanner = kind === PatternKind.Regexp ? [quoted('/'), ...baseQuotedScanner] : baseQuotedScanner

    const baseScanner = [operator, filter, ...quotedScanner, scanPattern(kind)]
    const tokenScanner: Scanner<Token>[] = interpretComments ? [comment, ...baseScanner] : baseScanner

    const baseEarlyPatternScanner = [...quotedScanner, scanBalancedPattern(kind)]
    const earlyPatternScanner = interpretComments ? [comment, ...baseEarlyPatternScanner] : baseEarlyPatternScanner

    return zeroOrMore(
        oneOf<Term>(
            whitespace,
            ...earlyPatternScanner.map(token => followedBy(token, whitespaceOrClosingParen)),
            openingParen,
            closingParen,
            ...tokenScanner.map(token => followedBy(token, whitespaceOrClosingParen))
        )
    )
}

/**
 * Scans a search query string.
 */
export const scanSearchQuery = (
    query: string,
    interpretComments?: boolean,
    kind = PatternKind.Literal
): ScanResult<Sequence> => {
    const scanner = createScanner(kind, interpretComments)
    return scanner(query, 0)
}
