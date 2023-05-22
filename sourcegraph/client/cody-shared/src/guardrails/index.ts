import { escapeMarkdown } from '@sourcegraph/common'

import { parseMarkdown } from '../chat/markdown'
import { isError } from '../utils'

export interface Attribution {
    repositories: RepositoryAttribution[]
}

export interface RepositoryAttribution {
    name: string
}

export interface Guardrails {
    searchAttribution(snippet: string): Promise<Attribution | Error>
}

/**
 * Returns markdown text with attribution information added in.
 *
 * @param guardrails client to use to lookup if a snippet of codes attributions
 * @param text markdown text
 */
export async function annotateAttribution(guardrails: Guardrails, text: string): Promise<string> {
    const tokens = parseMarkdown(text)
    const parts = await Promise.all(
        tokens.map(async token => {
            if (token.type !== 'code') {
                return token.raw
            }

            const msg = await guardrails.searchAttribution(token.text).then(summariseAttribution)

            return `${token.raw}\n<div title="guardrails">🛡️ ${escapeMarkdown(msg)}</div>`
        })
    )
    return parts.join('')
}

export function summariseAttribution(attribution: Attribution | Error): string {
    if (isError(attribution)) {
        return `guardrails attribution search failed: ${attribution.message}`
    }

    const repos = attribution.repositories
    const count = repos.length
    if (count === 0) {
        return 'no matching repositories found'
    }

    const summary = repos.slice(0, count < 5 ? count : 5).map(repo => repo.name)
    if (count > 5) {
        summary.push('...')
    }

    return `found ${count} matching repositories ${summary.join(', ')}`
}
