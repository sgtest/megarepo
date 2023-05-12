/* eslint-disable no-void */
import { useCallback, useEffect, useMemo, useRef } from 'react'

import { isEqual } from 'lodash'
import create from 'zustand'

import { Client, createClient, ClientInit, Transcript, TranscriptJSON } from '@sourcegraph/cody-shared/src/chat/client'
import { ChatContextStatus } from '@sourcegraph/cody-shared/src/chat/context'
import { ChatMessage } from '@sourcegraph/cody-shared/src/chat/transcript/messages'
import { PrefilledOptions } from '@sourcegraph/cody-shared/src/editor/withPreselectedOptions'
import { isErrorLike } from '@sourcegraph/common'

import { eventLogger } from '../../tracking/eventLogger'
import { EventName } from '../../util/constants'
import { CodeMirrorEditor } from '../components/CodeMirrorEditor'
import { useIsCodyEnabled } from '../useIsCodyEnabled'

import { EditorStore, useEditorStore } from './editor'

interface CodyChatStore {
    readonly client: Client | null
    readonly config: ClientInit['config'] | null
    readonly editor: CodeMirrorEditor | null
    readonly messageInProgress: ChatMessage | null
    readonly transcript: ChatMessage[]
    readonly transcriptHistory: TranscriptJSON[]
    readonly transcriptId: string | null
    // private, not used outside of this module
    onEvent: ((eventName: 'submit' | 'reset' | 'error') => void) | null
    initializeClient: (
        config: Required<ClientInit['config']>,
        editorStore: React.MutableRefObject<EditorStore>,
        onEvent: (eventName: 'submit' | 'reset' | 'error') => void
    ) => Promise<void>
    submitMessage: (text: string) => void
    editMessage: (text: string) => void
    executeRecipe: (
        recipeId: string,
        options?: {
            prefilledOptions?: PrefilledOptions
        }
    ) => Promise<void>
    reset: () => void
    getChatContext: () => ChatContextStatus
    loadTranscriptFromHistory: (id: string) => Promise<void>
    clearHistory: () => void
    deleteHistoryItem: (id: string) => void
}

const CODY_TRANSCRIPT_HISTORY_KEY = 'cody:transcript-history'
const SAVE_MAX_TRANSCRIPT_HISTORY = 20

export const safeTimestampToDate = (timestamp: string = ''): Date => {
    if (isNaN(new Date(timestamp) as any)) {
        return new Date()
    }

    return new Date(timestamp)
}

const sortSliceTranscriptHistory = (transcriptHistory: TranscriptJSON[]): TranscriptJSON[] =>
    transcriptHistory
        .sort(
            (a, b) =>
                (safeTimestampToDate(a.lastInteractionTimestamp) as any) -
                (safeTimestampToDate(b.lastInteractionTimestamp) as any)
        )
        .map(transcript => (transcript.id ? transcript : { ...transcript, id: Transcript.fromJSON(transcript).id }))
        .slice(0, SAVE_MAX_TRANSCRIPT_HISTORY)

export const useChatStoreState = create<CodyChatStore>((set, get): CodyChatStore => {
    const fetchTranscriptHistory = (): TranscriptJSON[] => {
        try {
            const json = JSON.parse(
                window.localStorage.getItem(CODY_TRANSCRIPT_HISTORY_KEY) || '[]'
            ) as TranscriptJSON[]

            if (!Array.isArray(json)) {
                return []
            }

            const sorted = sortSliceTranscriptHistory(json)
            saveTranscriptHistory(sorted)

            return sorted
        } catch {
            return []
        }
    }

    const saveTranscriptHistory = (transcriptHistory: TranscriptJSON[]): void => {
        const sorted = sortSliceTranscriptHistory(transcriptHistory)

        window.localStorage.setItem(CODY_TRANSCRIPT_HISTORY_KEY, JSON.stringify(sorted))
        set({ transcriptHistory: sorted })
    }

    const clearHistory = (): void => {
        const { client, onEvent } = get()
        if (client && !isErrorLike(client)) {
            onEvent?.('reset')
            void client.reset()
        }
        saveTranscriptHistory([])
    }

    const deleteHistoryItem = (id: string): void => {
        const { transcriptId } = get()
        const transcriptHistory = fetchTranscriptHistory()

        saveTranscriptHistory(transcriptHistory.filter(transcript => transcript.id !== id))

        if (transcriptId === id) {
            set({ transcript: [], transcriptId: null })
        }
    }

    const submitMessage = (text: string): void => {
        const { client, onEvent, getChatContext } = get()
        if (client && !isErrorLike(client)) {
            const { codebase, filePath } = getChatContext()
            eventLogger.log(EventName.CODY_SIDEBAR_SUBMIT, {
                repo: codebase,
                path: filePath,
                text,
            })
            onEvent?.('submit')
            void client.submitMessage(text)
        }
    }

    const editMessage = (text: string): void => {
        const { client, onEvent, getChatContext } = get()
        if (client && !isErrorLike(client)) {
            const { codebase, filePath } = getChatContext()
            eventLogger.log(EventName.CODY_SIDEBAR_EDIT, {
                repo: codebase,
                path: filePath,
                text,
            })
            onEvent?.('submit')
            client.transcript.removeLastInteraction()
            void client.submitMessage(text)
        }
    }

    const executeRecipe = async (
        recipeId: string,
        options?: {
            prefilledOptions?: PrefilledOptions
        }
    ): Promise<void> => {
        const { client, getChatContext, onEvent } = get()
        if (client && !isErrorLike(client)) {
            const { codebase, filePath } = getChatContext()
            eventLogger.log(EventName.CODY_SIDEBAR_RECIPE, { repo: codebase, path: filePath, recipeId })
            onEvent?.('submit')
            await client.executeRecipe(recipeId, options)
            eventLogger.log(EventName.CODY_SIDEBAR_RECIPE_EXECUTED, { repo: codebase, path: filePath, recipeId })
        }
        return Promise.resolve()
    }

    const reset = async (): Promise<void> => {
        const { client, onEvent } = get()
        const transcriptHistory = fetchTranscriptHistory()

        if (client && !isErrorLike(client)) {
            // push current transcript to transcript history and save
            const transcript = await client.transcript.toJSON()
            if (transcript.interactions.length && !transcriptHistory.find(({ id }) => id === transcript.id)) {
                transcriptHistory.push(transcript)
            }
            set({ messageInProgress: null, transcript: [] })
            saveTranscriptHistory(transcriptHistory)

            onEvent?.('reset')
            void client.reset()
        }
    }

    const setTranscript = async (transcript: Transcript): Promise<void> => {
        const { client } = get()
        if (!client || isErrorLike(client)) {
            return
        }

        const messages = transcript.toChat()
        if (client.isMessageInProgress) {
            messages.pop()
        }

        set({ transcript: messages, transcriptId: transcript.isEmpty ? null : transcript.id })

        if (transcript.isEmpty) {
            return
        }

        // find the transcript in history and update it
        const transcriptHistory = fetchTranscriptHistory()
        const transcriptJSONIndex = transcriptHistory.findIndex(({ id }) => id === transcript.id)
        if (transcriptJSONIndex === -1) {
            transcriptHistory.push(await transcript.toJSON())
        } else {
            transcriptHistory[transcriptJSONIndex] = await transcript.toJSON()
        }

        saveTranscriptHistory(transcriptHistory)
    }

    const setMessageInProgress = (message: ChatMessage | null): void => set({ messageInProgress: message })

    const initializeClient = async (
        config: Required<ClientInit['config']>,
        editorStateRef: React.MutableRefObject<EditorStore>,
        onEvent: (eventName: 'submit' | 'reset' | 'error') => void
    ): Promise<void> => {
        const editor = new CodeMirrorEditor(editorStateRef)

        const transcriptHistory = fetchTranscriptHistory()

        const initialTranscript = ((): Transcript => {
            try {
                return Transcript.fromJSON(transcriptHistory[transcriptHistory.length - 1] || { interactions: [] })
            } catch {
                return new Transcript()
            }
        })()

        set({
            config,
            editor,
            onEvent,
            transcript: initialTranscript.toChat(),
            transcriptId: initialTranscript.isEmpty ? null : initialTranscript.id,
            transcriptHistory,
        })

        try {
            const client = await createClient({
                config,
                editor,
                setMessageInProgress,
                initialTranscript,
                setTranscript: (transcript: Transcript) => void setTranscript(transcript),
            })

            set({ client })
        } catch (error) {
            eventLogger.log(EventName.CODY_SIDEBAR_CLIENT_ERROR, { repo: config?.codebase })
            onEvent('error')
            set({ client: error })
        }
    }

    const getChatContext = (): ChatContextStatus => {
        const { config, editor, client } = get()

        return {
            codebase: config?.codebase,
            filePath: editor?.getActiveTextEditorSelectionOrEntireFile()?.fileName,
            supportsKeyword: false,
            mode: config?.useContext,
            connection: client?.codebaseContext.checkEmbeddingsConnection(),
        }
    }

    const loadTranscriptFromHistory = async (id: string): Promise<void> => {
        const { client: oldClient, config, editor, onEvent } = get()
        if (!config || !editor) {
            return
        }

        if (oldClient && !isErrorLike(oldClient)) {
            oldClient.reset()
        }

        const transcriptHistory = fetchTranscriptHistory()
        const transcriptJSONFromHistory = transcriptHistory.find(json => json.id === id)
        if (!transcriptJSONFromHistory) {
            return
        }

        const transcript = Transcript.fromJSON(transcriptJSONFromHistory)
        const messages = transcript.toChat()

        try {
            const client = await createClient({
                config,
                editor,
                setMessageInProgress,
                initialTranscript: transcript,
                setTranscript: (transcript: Transcript) => void setTranscript(transcript),
            })

            set({ client, transcript: messages })
            await setTranscript(transcript)
        } catch (error) {
            eventLogger.log(EventName.CODY_SIDEBAR_CLIENT_ERROR, { repo: config?.codebase })
            onEvent?.('error')
            set({ client: error })
        }
    }

    return {
        client: null,
        editor: null,
        messageInProgress: null,
        config: null,
        transcript: [],
        transcriptHistory: fetchTranscriptHistory(),
        onEvent: null,
        transcriptId: null,
        initializeClient,
        submitMessage,
        editMessage,
        executeRecipe,
        reset: () => void reset(),
        getChatContext,
        loadTranscriptFromHistory,
        clearHistory,
        deleteHistoryItem,
    }
})

export const useChatStore = ({
    codebase,
    setIsCodySidebarOpen = () => undefined,
}: {
    codebase: string
    setIsCodySidebarOpen?: (state: boolean | undefined) => void
}): CodyChatStore => {
    const store = useChatStoreState()
    const enabled = useIsCodyEnabled()

    const onEvent = useCallback(
        (eventName: 'submit' | 'reset' | 'error') => {
            if (eventName === 'submit') {
                setIsCodySidebarOpen(true)
            }
        },
        [setIsCodySidebarOpen]
    )

    // We use a ref here so that a change in the editor state does not need a recreation of the
    // client config.
    const editorStore = useEditorStore()
    const editorStateRef = useRef(editorStore)
    useEffect(() => {
        editorStateRef.current = editorStore
    }, [editorStore])

    // TODO(naman): change useContext to `blended` after adding keyboard context
    const config = useMemo<Required<ClientInit['config']>>(
        () => ({
            serverEndpoint: window.location.origin,
            useContext: 'embeddings',
            codebase,
            accessToken: null,
            customHeaders: window.context.xhrHeaders,
        }),
        [codebase]
    )

    const { initializeClient, config: currentConfig } = store
    useEffect(() => {
        if (!(enabled.chat || enabled.sidebar) || isEqual(config, currentConfig)) {
            return
        }

        void initializeClient(config, editorStateRef, onEvent)
    }, [config, initializeClient, currentConfig, editorStateRef, onEvent, enabled.chat, enabled.sidebar])

    return store
}
