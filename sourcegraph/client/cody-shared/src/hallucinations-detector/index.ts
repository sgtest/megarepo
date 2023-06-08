import { marked } from 'marked'

import { parseMarkdown } from '../chat/markdown'

export interface HighlightedToken {
    type: 'file' | 'symbol'
    // Including leading/trailing whitespaces or quotes.
    outerValue: string
    innerValue: string
    isHallucinated: boolean
}

interface HighlightTokensResult {
    text: string
    tokens: HighlightedToken[]
}

export async function highlightTokens(
    text: string,
    filesExist: (filePaths: string[]) => Promise<{ [filePath: string]: boolean }>,
    workspaceRootPath?: string
): Promise<HighlightTokensResult> {
    const markdownTokens = parseMarkdown(text)
    const tokens = await detectTokens(markdownTokens, filesExist)

    const highlightedText = markdownTokens
        .map(token => {
            switch (token.type) {
                case 'code':
                case 'codespan':
                    return token.raw
                default:
                    return highlightLine(token.raw, tokens, workspaceRootPath)
            }
        })
        .join('')

    return { text: highlightedText, tokens }
}

async function detectTokens(
    tokens: marked.Token[],
    filesExist: (filePaths: string[]) => Promise<{ [filePath: string]: boolean }>
): Promise<HighlightedToken[]> {
    // mapping from file path to full match
    const filePathToFullMatch: { [filePath: string]: Set<string> } = {}
    for (const token of tokens) {
        switch (token.type) {
            case 'code':
            case 'codespan':
                continue
            default: {
                const lines = token.raw.split('\n')
                for (const line of lines) {
                    for (const { fullMatch, pathMatch } of findFilePaths(line)) {
                        if (!filePathToFullMatch[pathMatch]) {
                            filePathToFullMatch[pathMatch] = new Set<string>()
                        }
                        filePathToFullMatch[pathMatch].add(fullMatch)
                    }
                }
            }
        }
    }

    const filePathsExist = await filesExist([...Object.keys(filePathToFullMatch)])
    const highlightedTokens: HighlightedToken[] = []
    for (const [filePath, fullMatches] of Object.entries(filePathToFullMatch)) {
        const exists = filePathsExist[filePath.endsWith('/') ? filePath.slice(0, -1) : filePath]
        for (const fullMatch of fullMatches) {
            highlightedTokens.push({
                type: 'file',
                outerValue: fullMatch,
                innerValue: filePath,
                isHallucinated: !exists,
            })
        }
    }
    return highlightedTokens
}

function highlightLine(line: string, tokens: HighlightedToken[], workspaceRootPath?: string): string {
    let highlightedLine = line
    for (const token of tokens) {
        highlightedLine = highlightedLine.replaceAll(
            token.outerValue,
            getHighlightedTokenHTML(token, workspaceRootPath)
        )
    }
    return highlightedLine
}

function getHighlightedTokenHTML(token: HighlightedToken, workspaceRootPath?: string): string {
    let filePath = token.outerValue.trim()
    // Create workspace relative links for existing files (excluding directories)
    if (!token.isHallucinated && workspaceRootPath && filePath.includes('.')) {
        // Need to decode the file path because it's encoded in the markdown
        filePath = decodeURIComponent(filePath.replace(/["'`]/g, ''))
        const fileUri = `vscode://file${workspaceRootPath}/${filePath}`
        const uri = new URL(fileUri).href
        filePath = `<a href="${uri}">${filePath}</a>`
    }
    const isHallucinatedClassName = token.isHallucinated ? 'hallucinated' : 'not-hallucinated'
    return ` <span class="token-${token.type} token-${isHallucinatedClassName}">${filePath}</span> `
}

export function findFilePaths(line: string): { fullMatch: string; pathMatch: string }[] {
    const matches: { fullMatch: string; pathMatch: string }[] = []
    for (const m of line.matchAll(filePathRegexp)) {
        const fullMatch = m[0]
        const pathMatch = m[1]
        if (isFilePathLike(fullMatch, pathMatch)) {
            matches.push({ fullMatch, pathMatch })
        }
    }
    return matches
}

const filePathCharacters = '[\\@\\*\\w\\/\\._-]'

const filePathRegexpParts = [
    // File path can start with a `, ", ', or a whitespace
    '[`"\'\\s]?',
    // Capture a file path-like sequence.
    `(\\/?${filePathCharacters}+\\/${filePathCharacters}+)`,
    //  File path can end with a `, ", ', ., or a whitespace.
    '[`"\'\\s\\.]?',
]

const filePathRegexp = new RegExp(filePathRegexpParts.join(''), 'g')

function isFilePathLike(fullMatch: string, pathMatch: string): boolean {
    if (
        fullMatch.length >= 1 &&
        (['"', "'", '`'].includes(fullMatch.charAt(0)) ||
            ['"', "'", '`'].includes(fullMatch.charAt(fullMatch.length - 1)))
    ) {
        if (!fullMatch.endsWith(fullMatch.charAt(0))) {
            // unbalanced delimiters
            return false
        }
    }

    const parts = pathMatch.split(/[/\\]/)
    if (pathMatch.includes('*')) {
        // Probably a glob pattern
        return false
    }
    if (parts.length === 2 && pathMatch.startsWith('@')) {
        // Probably an npm package
        return false
    }

    if (fullMatch.startsWith(' ') && parts.length <= 2) {
        // Probably a / used as an "or" in a sentence. For example, "This is a cool/awesome function."
        return false
    }

    if (parts[0].includes('.com') || parts[0].startsWith('http')) {
        // Probably a URL.
        return false
    }
    // TODO: we can do further validation here.
    return true
}
