import * as anthropic from '@anthropic-ai/sdk'

import { SourcegraphNodeCompletionsClient } from '@sourcegraph/cody-shared/src/sourcegraph-api/completions/nodeClient'
import {
    CompletionParameters,
    CompletionResponse,
    Message,
} from '@sourcegraph/cody-shared/src/sourcegraph-api/completions/types'

import { Completion } from '.'
import { ReferenceSnippet } from './context'
import { messagesToText } from './prompts'

export abstract class CompletionProvider {
    constructor(
        protected completionsClient: SourcegraphNodeCompletionsClient,
        protected promptChars: number,
        protected responseTokens: number,
        protected snippets: ReferenceSnippet[],
        protected prefix: string,
        protected suffix: string,
        protected injectPrefix: string,
        protected defaultN: number = 1
    ) {}

    // Returns the content specific prompt excluding additional referenceSnippets
    protected abstract createPromptPrefix(): Message[]

    public emptyPromptLength(): number {
        const promptNoSnippets = messagesToText(this.createPromptPrefix())
        return promptNoSnippets.length - 10 // extra 10 chars of buffer cuz who knows
    }

    // Creates the resulting prompt and adds as many snippets from the reference
    // list as possible.
    protected createPrompt(): Message[] {
        const prefixMessages = this.createPromptPrefix()
        const referenceSnippetMessages: Message[] = []

        let remainingChars = this.promptChars - this.emptyPromptLength()

        if (this.suffix.length > 0) {
            let suffix = ''
            // We throw away the first 5 lines of the suffix to avoid the LLM to
            // just continue the completion by appending the suffix.
            const suffixLines = this.suffix.split('\n')
            if (suffixLines.length > 5) {
                suffix = suffixLines.slice(5).join('\n')
            }

            if (suffix.length > 0) {
                const suffixContext: Message[] = [
                    {
                        speaker: 'human',
                        text:
                            'Add the following code snippet to your knowledge base:\n' +
                            '```' +
                            `\n${suffix}\n` +
                            '```',
                    },
                    {
                        speaker: 'assistant',
                        text: 'Okay, I have added it to my knowledge base.',
                    },
                ]

                const numSnippetChars = messagesToText(suffixContext).length + 1
                if (numSnippetChars <= remainingChars) {
                    referenceSnippetMessages.push(...suffixContext)
                    remainingChars -= numSnippetChars
                }
            }
        }

        for (const snippet of this.snippets) {
            const snippetMessages: Message[] = [
                {
                    speaker: 'human',
                    text:
                        `Add the following code snippet (from file ${snippet.filename}) to your knowledge base:\n` +
                        '```' +
                        `\n${snippet.text}\n` +
                        '```',
                },
                {
                    speaker: 'assistant',
                    text: 'Okay, I have added it to my knowledge base.',
                },
            ]
            const numSnippetChars = messagesToText(snippetMessages).length + 1
            if (numSnippetChars > remainingChars) {
                break
            }
            referenceSnippetMessages.push(...snippetMessages)
            remainingChars -= numSnippetChars
        }

        return [...referenceSnippetMessages, ...prefixMessages]
    }

    public abstract generateCompletions(abortSignal: AbortSignal, n?: number): Promise<Completion[]>
}

export class MultilineCompletionProvider extends CompletionProvider {
    protected createPromptPrefix(): Message[] {
        // TODO(beyang): escape 'Human:' and 'Assistant:'
        const prefix = this.prefix.trim()

        const prefixLines = prefix.split('\n')
        if (prefixLines.length === 0) {
            throw new Error('no prefix lines')
        }

        let prefixMessages: Message[]
        if (prefixLines.length > 2) {
            const endLine = Math.max(Math.floor(prefixLines.length / 2), prefixLines.length - 5)
            prefixMessages = [
                {
                    speaker: 'human',
                    text:
                        'Complete the following file:\n' +
                        '```' +
                        `\n${prefixLines.slice(0, endLine).join('\n')}\n` +
                        '```',
                },
                {
                    speaker: 'assistant',
                    text: `Here is the completion of the file:\n\`\`\`\n${prefixLines.slice(endLine).join('\n')}`,
                },
            ]
        } else {
            prefixMessages = [
                {
                    speaker: 'human',
                    text: 'Write some code',
                },
                {
                    speaker: 'assistant',
                    text: `Here is some code:\n\`\`\`\n${prefix}`,
                },
            ]
        }

        return prefixMessages
    }

    private postProcess(completion: string): string {
        let suggestion = completion
        const endBlockIndex = completion.indexOf('```')
        if (endBlockIndex !== -1) {
            suggestion = completion.slice(0, endBlockIndex)
        }

        // Remove trailing whitespace before newlines
        suggestion = suggestion
            .split('\n')
            .map(line => line.trimEnd())
            .join('\n')

        return sliceUntilFirstNLinesOfSuffixMatch(suggestion, this.suffix, 5)
    }

    public async generateCompletions(abortSignal: AbortSignal, n?: number): Promise<Completion[]> {
        const prefix = this.prefix.trim()

        // Create prompt
        const prompt = this.createPrompt()
        const textPrompt = messagesToText(prompt)
        if (textPrompt.length > this.promptChars) {
            throw new Error('prompt length exceeded maximum alloted chars')
        }

        // Issue request
        const responses = await batchCompletions(
            this.completionsClient,
            {
                messages: prompt,
                maxTokensToSample: this.responseTokens,
            },
            n || this.defaultN,
            abortSignal
        )
        // Post-process
        return responses.map(resp => ({
            prefix,
            messages: prompt,
            content: this.postProcess(resp.completion),
            stopReason: resp.stopReason,
        }))
    }
}

export class EndOfLineCompletionProvider extends CompletionProvider {
    constructor(
        completionsClient: SourcegraphNodeCompletionsClient,
        promptChars: number,
        responseTokens: number,
        snippets: ReferenceSnippet[],
        prefix: string,
        suffix: string,
        injectPrefix: string,
        defaultN: number = 1,
        protected multiline: boolean = false
    ) {
        super(completionsClient, promptChars, responseTokens, snippets, prefix, suffix, injectPrefix, defaultN)
    }

    protected createPromptPrefix(): Message[] {
        // TODO(beyang): escape 'Human:' and 'Assistant:'
        const prefixLines = this.prefix.split('\n')
        if (prefixLines.length === 0) {
            throw new Error('no prefix lines')
        }

        let prefixMessages: Message[]
        if (prefixLines.length > 2) {
            const endLine = Math.max(Math.floor(prefixLines.length / 2), prefixLines.length - 5)
            prefixMessages = [
                {
                    speaker: 'human',
                    text:
                        'Complete the following file:\n' +
                        '```' +
                        `\n${prefixLines.slice(0, endLine).join('\n')}\n` +
                        '```',
                },
                {
                    speaker: 'assistant',
                    text:
                        'Here is the completion of the file:\n' +
                        '```' +
                        `\n${prefixLines.slice(endLine).join('\n')}${this.injectPrefix}`,
                },
            ]
        } else {
            prefixMessages = [
                {
                    speaker: 'human',
                    text: 'Write some code',
                },
                {
                    speaker: 'assistant',
                    text: `Here is some code:\n\`\`\`\n${this.prefix}${this.injectPrefix}`,
                },
            ]
        }

        return prefixMessages
    }

    private postProcess(completion: string): string {
        // Sometimes Claude emits an extra space
        if (
            completion.length > 0 &&
            completion.startsWith(' ') &&
            this.prefix.length > 0 &&
            this.prefix.endsWith(' ')
        ) {
            completion = completion.slice(1)
        }
        // Insert the injected prefix back in
        if (this.injectPrefix.length > 0) {
            completion = this.injectPrefix + completion
        }
        // Strip out trailing markdown block and trim trailing whitespace
        const endBlockIndex = completion.indexOf('```')
        if (endBlockIndex !== -1) {
            return completion.slice(0, endBlockIndex).trimEnd()
        }

        if (this.multiline) {
            // We use a whitespace counting approach to finding the end of the completion. To find
            // an end, we look for the first line that is below the start scope of the completion (
            // calculated by the number of leading spaces or tabs)

            const prefixLastLineIndent = this.prefix.length - this.prefix.lastIndexOf('\n') - 1
            const completionFirstLineIndent = indentation(completion)
            const startIndent = prefixLastLineIndent + completionFirstLineIndent

            const lines = completion.split('\n')
            let cutOffIndex = lines.length
            for (let i = 0; i < lines.length; i++) {
                const line = lines[i]

                if (i === 0 || line === '' || line.trim().startsWith('} else')) {
                    continue
                }

                if (indentation(line) < startIndent) {
                    // When we find the first block below the start indentation, only include it if
                    // it is an end block
                    if (line.trim() === '}') {
                        cutOffIndex = i + 1
                    } else {
                        cutOffIndex = i
                    }
                    break
                }
            }

            completion = lines.slice(0, cutOffIndex).join('\n')
        }

        return completion.trimEnd()
    }

    public async generateCompletions(abortSignal: AbortSignal, n?: number): Promise<Completion[]> {
        const prefix = this.prefix + this.injectPrefix

        // Create prompt
        const prompt = this.createPrompt()
        if (prompt.length > this.promptChars) {
            throw new Error('prompt length exceeded maximum alloted chars')
        }

        // Issue request
        const responses = await batchCompletions(
            this.completionsClient,
            {
                messages: prompt,
                stopSequences: this.multiline ? [anthropic.HUMAN_PROMPT, '\n\n\n'] : [anthropic.HUMAN_PROMPT, '\n'],
                maxTokensToSample: this.responseTokens,
                temperature: 1,
                topK: -1,
                topP: -1,
            },
            n || this.defaultN,
            abortSignal
        )
        // Post-process
        return responses.map(resp => ({
            prefix,
            messages: prompt,
            content: this.postProcess(resp.completion),
            stopReason: resp.stopReason,
        }))
    }
}

async function batchCompletions(
    client: SourcegraphNodeCompletionsClient,
    params: CompletionParameters,
    n: number,
    abortSignal: AbortSignal
): Promise<CompletionResponse[]> {
    const responses: Promise<CompletionResponse>[] = []
    for (let i = 0; i < n; i++) {
        responses.push(client.complete(params, abortSignal))
    }
    return Promise.all(responses)
}

/**
 * This function slices the suggestion string until the first n lines match the suffix string.
 *
 * It splits suggestion and suffix into lines, then iterates over the lines of suffix. For each line
 * of suffix, it checks if the next n lines of suggestion match. If so, it returns the first part of
 * suggestion up to those matching lines. If no match is found after iterating all lines of suffix,
 * the full suggestion is returned.
 *
 * For example, with:
 * suggestion = "foo\nbar\nbaz\nqux\nquux"
 * suffix = "baz\nqux\nquux"
 * n = 3
 *
 * It would return: "foo\nbar"
 *
 * Because the first 3 lines of suggestion ("baz\nqux\nquux") match suffix.
 */
export function sliceUntilFirstNLinesOfSuffixMatch(suggestion: string, suffix: string, n: number): string {
    const suggestionLines = suggestion.split('\n')
    const suffixLines = suffix.split('\n')

    for (let i = 0; i < suffixLines.length; i++) {
        let matchedLines = 0
        for (let j = 0; j < suggestionLines.length; j++) {
            if (suffixLines.length < i + matchedLines) {
                continue
            }
            if (suffixLines[i + matchedLines] === suggestionLines[j]) {
                matchedLines += 1
            } else {
                matchedLines = 0
            }
            if (matchedLines >= n) {
                return suggestionLines.slice(0, j - n + 1).join('\n')
            }
        }
    }

    return suggestion
}

function indentation(line: string): number {
    const regex = line.match(/^[\t ]*/)
    return regex ? regex[0].length : 0
}
