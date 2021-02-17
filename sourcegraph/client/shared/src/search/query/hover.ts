import * as Monaco from 'monaco-editor'
import { Token } from './token'
import {
    decorate,
    toMonacoRange,
    DecoratedToken,
    MetaRegexp,
    MetaRegexpKind,
    MetaRevision,
    MetaGitRevision,
    MetaSourcegraphRevision,
    MetaStructural,
    MetaStructuralKind,
    MetaSelector,
    MetaSelectorKind,
} from './decoratedToken'
import { resolveFilter } from './filters'

const toRegexpHover = (token: MetaRegexp): string => {
    switch (token.kind) {
        case MetaRegexpKind.Alternative:
            return '**Or**. Match either the expression before or after the `|`.'
        case MetaRegexpKind.Assertion:
            switch (token.value) {
                case '^':
                    return '**Start anchor**. Match the beginning of a string. Typically used to match a string prefix, as in `^prefix`. Also often used with the end anchor `$` to match an exact string, as in `^exact$`.'
                case '$':
                    return '**End anchor**. Match the end of a string. Typically used to match a string suffix, as in `suffix$`. Also often used with the start anchor to match an exact string, as in `^exact$`.'
                case '\\b':
                    return '**Word boundary**. Match a position where a word character comes after a non-word character, or vice versa. Typically used to match whole words, as in `\\bword\\b`.'
                case '\\B':
                    return '**Negated word boundary**. Match a position between two word characters, or a position between two non-word characters. This is the negation of `\\b`.'
            }
        case MetaRegexpKind.CharacterClass:
            return token.value.startsWith('[^')
                ? '**Negated character class**. Match any character _not_ inside the square brackets.'
                : '**Character class**. Match any character inside the square brackets.'
        case MetaRegexpKind.CharacterSet:
            switch (token.value) {
                case '.':
                    return '**Dot**. Match any character except a line break.'
                case '\\w':
                    return '**Word**. Match any word character. '
                case '\\W':
                    return '**Negated word**. Match any non-word character. Matches any character that is **not** an alphabetic character, digit, or underscore.'
                case '\\d':
                    return '**Digit**. Match any digit character `0-9`.'
                case '\\D':
                    return '**Negated digit**. Match any character that is **not** a digit `0-9`.'
                case '\\s':
                    return '**Whitespace**. Match any whitespace character like a space, line break, or tab.'
                case '\\S':
                    return '**Negated whitespace**. Match any character that is **not** a whitespace character like a space, line break, or tab.'
            }
        case MetaRegexpKind.CharacterClassRange:
        case MetaRegexpKind.CharacterClassRangeHyphen:
            return `**Character range**. Match a character in the range \`${token.value}\`.`
        case MetaRegexpKind.CharacterClassMember:
            return `**Character**. This character class matches the character \`${token.value}\`.`
        case MetaRegexpKind.Delimited:
            return '**Group**. Groups together multiple expressions to match.'
        case MetaRegexpKind.EscapedCharacter: {
            const escapable = '~`!@#$%^&*()[]{}<>,.?/\\|=+-_'
            let description = escapable.includes(token.value[1])
                ? `Match the character \`${token.value[1]}\`.`
                : `The character \`${token.value[1]}\` is escaped.`
            switch (token.value[1]) {
                case 'n':
                    description = 'Match a new line.'
                    break
                case 't':
                    description = 'Match a tab.'
                    break
                case 'r':
                    description = 'Match a carriage return.'
                    break
            }
            return `**Escaped Character**. ${description}`
        }
        case MetaRegexpKind.LazyQuantifier:
            return '**Lazy**. Match as few as characters as possible that match the previous expression.'
        case MetaRegexpKind.RangeQuantifier:
            switch (token.value) {
                case '*':
                    return '**Zero or more**. Match zero or more of the previous expression.'
                case '?':
                    return '**Optional**. Match zero or one of the previous expression.'
                case '+':
                    return '**One or more**. Match one or more of the previous expression.'
                default: {
                    const range = token.value.slice(1, -1).split(',')
                    let quantity = ''
                    if (range.length === 1 || (range.length === 2 && range[0] === range[1])) {
                        quantity = range[0]
                    } else if (range[1] === '') {
                        quantity = `${range[0]} or more`
                    } else {
                        quantity = `between ${range[0]} and ${range[1]}`
                    }
                    return `**Range**. Match ${quantity} of the previous expression.`
                }
            }
    }
}

const toStructuralHover = (token: MetaStructural): string => {
    switch (token.kind) {
        case MetaStructuralKind.Hole:
            return '**Structural hole**. Matches code structures contextually. See the [syntax reference](https://docs.sourcegraph.com/code_search/reference/structural#syntax-reference) for a complete description.'
        case MetaStructuralKind.RegexpHole:
            return '**Regular expression hole**. Match the regular expression defined inside this hole.'
        case MetaStructuralKind.Variable:
            return '**Hole variable**. A descriptive name for the syntax matched by this hole.'
        case MetaStructuralKind.RegexpSeparator:
            return '**Regular expression separator**. Indicates the start of a regular expression that this hole should match.'
    }
}

const toRevisionHover = (token: MetaRevision): string => {
    switch (token.kind) {
        case MetaGitRevision.CommitHash:
            return '**Revision commit hash**. Search the repository at this commit.'
        case MetaGitRevision.Label:
            if (token.value.match(/^head$/i)) {
                return '**Revision HEAD**. Search the repository at the latest HEAD commit of the default branch.'
            }
            return '**Revision branch name or tag**. Search the branch name or tag at the head commit.'
        case MetaGitRevision.ReferencePath:
            return '**Revision using git reference path**. Search the branch name or tag at the head commit. Search across git objects, like commits or branches, that match this git reference path. Typically used in conjunction with glob patterns, where a pattern like `*refs/heads/*` searches across all repository branches at the head commit.'
        case MetaGitRevision.Wildcard:
            return '**Revision wildcard**. Glob syntax to match zero or more characters in a revision. Typically used to match multiple branches or tags based on a git reference path. For example, `refs/tags/v3.*` matches all tags that start with `v3.`.'
        case MetaSourcegraphRevision.IncludeGlobMarker:
            return '**Revision glob pattern to include**. A prefixing indicating that a glob pattern follows. Git references matching the glob pattern are included in the search. Typically used where a pattern like `*refs/heads/*` searches across all repository branches at the head commit.'
        case MetaSourcegraphRevision.ExcludeGlobMarker:
            return '**Revision glob pattern to exclude**. A prefix indicating that git references, like a commit or branch name, should be **excluded** from search based on the glob pattern that follows. Used in conjunction with a glob pattern that matches a set of commits or branches, followed by a a pattern to exclude from the set. For example, `*refs/heads/*:*!refs/heads/release*` searches all branches at the head commit, excluding branches matching `release*`.'
        case MetaSourcegraphRevision.Separator:
            return '**Revision separator**. Separates multiple revisions to search across. For example, `1a35d48:feature:3.15` searches the repository for matches at commit `1a35d48`, or a branch named `feature`, or a tag `3.15`.'
    }
}

const toSelectorHover = (token: MetaSelector): string => {
    switch (token.kind) {
        case MetaSelectorKind.Repo:
            return 'Select and display distinct repository paths from search results.'
        case MetaSelectorKind.File:
            return 'Select and display distinct file paths from search results.'
        case MetaSelectorKind.Content:
            return 'Select and display only results matching content inside files.'
        case MetaSelectorKind.Commit:
            return 'Select and display only commit data of the result. Must be used in conjunction with commit search, i.e., `type:commit`.'
        case MetaSelectorKind.Symbol:
            return 'Select and display only symbol data of the result. Must be used in conjunction with a symbol search, i.e., `type:symbol`.'
    }
}

const toHover = (token: DecoratedToken): string => {
    switch (token.type) {
        case 'pattern': {
            const quantity = token.value.length > 1 ? 'string' : 'character'
            return `Matches the ${quantity} \`${token.value}\`.`
        }
        case 'metaRegexp':
            return toRegexpHover(token)
        case 'metaRevision':
            return toRevisionHover(token)
        case 'metaRepoRevisionSeparator':
            return '**Search at revision**. Separates a repository pattern and the revisions to search, like commits or branches. The part before the `@` specifies the repositories to search, the part after the `@` specifies which revisions to search.'
        case 'metaSelector':
            return toSelectorHover(token)
        case 'metaStructural':
            return toStructuralHover(token)
    }
    return ''
}

const inside = (column: number) => ({ range }: Pick<Token | DecoratedToken, 'range'>): boolean =>
    range.start + 1 <= column && range.end >= column

/**
 * Returns the hover result for a hovered search token in the Monaco query input.
 */
export const getHoverResult = (
    tokens: Token[],
    { column }: Pick<Monaco.Position, 'column'>,
    smartQuery = false
): Monaco.languages.Hover | null => {
    const tokensAtCursor = (smartQuery ? tokens.flatMap(decorate) : tokens).filter(inside(column))
    if (tokensAtCursor.length === 0) {
        return null
    }
    const values: string[] = []
    let range: Monaco.IRange | undefined
    tokensAtCursor.map(token => {
        switch (token.type) {
            case 'filter': {
                // This 'filter' branch only exists to preserve previous behavior when smmartQuery is false.
                // When smartQuery is true, 'filter' tokens are handled by the 'field' case and its values in
                // the rest of this switch statement.
                const resolvedFilter = resolveFilter(token.field.value)
                if (resolvedFilter) {
                    values.push(
                        'negated' in resolvedFilter
                            ? resolvedFilter.definition.description(resolvedFilter.negated)
                            : resolvedFilter.definition.description
                    )
                    range = toMonacoRange(token.range)
                }
                break
            }
            case 'field': {
                const resolvedFilter = resolveFilter(token.value)
                if (resolvedFilter) {
                    values.push(
                        'negated' in resolvedFilter
                            ? resolvedFilter.definition.description(resolvedFilter.negated)
                            : resolvedFilter.definition.description
                    )
                    // Add 1 to end of range to include the ':'.
                    range = toMonacoRange({ start: token.range.start, end: token.range.end + 1 })
                }
                break
            }
            case 'pattern':
            case 'metaRevision':
            case 'metaRepoRevisionSeparator':
            case 'metaSelector':
                values.push(toHover(token))
                range = toMonacoRange(token.range)
                break
            case 'metaRegexp':
                values.push(toHover(token))
                range = toMonacoRange(token.groupRange ? token.groupRange : token.range)
                break
            case 'metaStructural':
                values.push(toHover(token))
                range = toMonacoRange(token.groupRange ? token.groupRange : token.range)
        }
    })
    return {
        contents: values.map<Monaco.IMarkdownString>(
            (value): Monaco.IMarkdownString => ({
                value,
            })
        ),
        range,
    }
}
