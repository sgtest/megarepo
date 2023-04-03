import { fetchEventSource } from '@microsoft/fetch-event-source'

import { SourcegraphCompletionsClient } from './client'
import type { Event, CompletionParameters, CompletionCallbacks } from './types'

export class SourcegraphBrowserCompletionsClient extends SourcegraphCompletionsClient {
    public stream(params: CompletionParameters, cb: CompletionCallbacks): () => void {
        const abort = new AbortController()
        fetchEventSource(this.completionsEndpoint, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json; charset=utf-8',
                ...(this.accessToken ? { Authorization: `token ${this.accessToken}` } : null),
            },
            body: JSON.stringify(params),
            signal: abort.signal,
            onmessage: message => {
                // console.log('[EventSource]', message)
                const data: Event = { ...JSON.parse(message.data), type: message.event }
                this.sendEvents([data], cb)
            },
            onerror(error) {
                console.error(error)
            },
        }).catch(error => {
            console.error(error)
        })
        return () => abort.abort()
    }
}
