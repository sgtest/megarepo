import type { ConfigurationWithAccessToken } from '../../configuration'
import type { FeatureFlagProvider } from '../../experimentation/FeatureFlagProvider'

import type { CompletionCallbacks, CompletionParameters, CompletionResponse, Event } from './types'

export interface CompletionLogger {
    startCompletion(params: CompletionParameters | {}):
        | undefined
        | {
              onError: (error: string) => void
              onComplete: (response: string | CompletionResponse | string[] | CompletionResponse[]) => void
              onEvents: (events: Event[]) => void
          }
}

export type CompletionsClientConfig = Pick<
    ConfigurationWithAccessToken,
    'serverEndpoint' | 'accessToken' | 'debugEnable' | 'customHeaders'
>

/**
 * Access the chat based LLM APIs via a Sourcegraph server instance.
 */
export abstract class SourcegraphCompletionsClient {
    private errorEncountered = false

    constructor(
        protected config: CompletionsClientConfig,
        protected featureFlagProvider?: FeatureFlagProvider,
        protected logger?: CompletionLogger
    ) {}

    public onConfigurationChange(newConfig: CompletionsClientConfig): void {
        this.config = newConfig
    }

    protected get completionsEndpoint(): string {
        const url = new URL('/.api/completions/stream', this.config.serverEndpoint)

        // Sourcegraph >=5.4 instances require client name and version params on the completions endpoint to ensure client supports Cody Ignore functionality.
        // Ensure client name is always set to "web" for Cody Web. Client version is not required for Cody Web as it aligns with server version.
        // See https://github.com/sourcegraph/sourcegraph/pull/62048.
        url.searchParams.set('client-name', 'web')

        return url.href
    }

    protected sendEvents(events: Event[], cb: CompletionCallbacks): void {
        for (const event of events) {
            switch (event.type) {
                case 'completion': {
                    cb.onChange(event.completion)
                    break
                }
                case 'error': {
                    this.errorEncountered = true
                    cb.onError(event.error)
                    break
                }
                case 'done': {
                    if (!this.errorEncountered) {
                        cb.onComplete()
                    }
                    break
                }
            }
        }
    }

    public abstract stream(params: CompletionParameters, cb: CompletionCallbacks): () => void
}

/**
 * A helper function that calls the streaming API but will buffer the result
 * until the stream has completed.
 */
export function bufferStream(
    client: Pick<SourcegraphCompletionsClient, 'stream'>,
    params: CompletionParameters
): Promise<string> {
    return new Promise((resolve, reject) => {
        let buffer = ''
        const callbacks: CompletionCallbacks = {
            onChange(text: string) {
                buffer = text
            },
            onComplete() {
                resolve(buffer)
            },
            onError(message: string, code?: number) {
                reject(new Error(code ? `${message} (code ${code})` : message))
            },
        }
        client.stream(params, callbacks)
    })
}
