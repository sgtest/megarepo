/* eslint-disable no-void */
import { useCallback, useEffect, useMemo, useRef } from 'react'

import { isEqual } from 'lodash'
import create from 'zustand'

import { Client, createClient, ClientInit, Transcript, TranscriptJSON } from '@sourcegraph/cody-shared/src/chat/client'
import { ChatContextStatus } from '@sourcegraph/cody-shared/src/chat/context'
import { RecipeID } from '@sourcegraph/cody-shared/src/chat/recipes/recipe'
import { ChatMessage } from '@sourcegraph/cody-shared/src/chat/transcript/messages'
import { PrefilledOptions } from '@sourcegraph/cody-shared/src/editor/withPreselectedOptions'
import { isErrorLike } from '@sourcegraph/common'

import { eventLogger } from '../../tracking/eventLogger'
import { EventName } from '../../util/constants'
import { CodeMirrorEditor } from '../components/CodeMirrorEditor'
import { useIsCodyEnabled, isEmailVerificationNeeded } from '../useIsCodyEnabled'

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
        recipeId: RecipeID,
        options?: {
            prefilledOptions?: PrefilledOptions
        }
    ) => Promise<void>
    reset: () => Promise<void>
    getChatContext: () => ChatContextStatus
    loadTranscriptFromHistory: (id: string) => Promise<void>
    clearHistory: () => void
    deleteHistoryItem: (id: string) => void
}

const CODY_TRANSCRIPT_HISTORY_KEY = 'cody:transcript-history'
const CODY_CURRENT_TRANSCRIPT_ID_KEY = 'cody:current-transcript-id'
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
    const needsEmailVerification = isEmailVerificationNeeded()
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

    const fetchCurrentTranscriptId = (): string | null =>
        window.localStorage.getItem(CODY_CURRENT_TRANSCRIPT_ID_KEY) || null

    const setCurrentTranscriptId = (id: string | null): void => {
        window.localStorage.setItem(CODY_CURRENT_TRANSCRIPT_ID_KEY, id || '')
        set({ transcriptId: id })
    }

    const clearHistory = (): void => {
        if (needsEmailVerification) {
            return
        }

        const { client, onEvent } = get()
        saveTranscriptHistory([])
        if (client && !isErrorLike(client)) {
            onEvent?.('reset')
            void client.reset()
        }
    }

    const deleteHistoryItem = (id: string): void => {
        if (needsEmailVerification) {
            return
        }

        const { transcriptId } = get()
        const transcriptHistory = fetchTranscriptHistory()

        saveTranscriptHistory(transcriptHistory.filter(transcript => transcript.id !== id))

        if (transcriptId === id) {
            setCurrentTranscriptId(null)
            set({ transcript: [] })
        }
    }

    const submitMessage = (text: string): void => {
        const { client, onEvent, getChatContext } = get()

        if (needsEmailVerification) {
            onEvent?.('submit')
            return
        }

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

        if (needsEmailVerification) {
            onEvent?.('submit')
            return
        }

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
        recipeId: RecipeID,
        options?: {
            prefilledOptions?: PrefilledOptions
        }
    ): Promise<void> => {
        const { client, getChatContext, onEvent } = get()

        if (needsEmailVerification) {
            onEvent?.('submit')
            return
        }

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
        const { client: oldClient, config, editor, onEvent } = get()
        if (!config || !editor) {
            return
        }

        if (needsEmailVerification) {
            onEvent?.('submit')
            return
        }

        if (oldClient && !isErrorLike(oldClient)) {
            oldClient.reset()
        }

        const transcriptHistory = fetchTranscriptHistory()
        const transcript = new Transcript()
        const messages = transcript.toChat()
        saveTranscriptHistory([...transcriptHistory, await transcript.toJSON()])

        try {
            const client = await createClient({
                config: { ...config, customHeaders: window.context.xhrHeaders },
                editor,
                setMessageInProgress,
                initialTranscript: transcript,
                setTranscript: (transcript: Transcript) => void setTranscript(transcript),
            })

            setCurrentTranscriptId(transcript.id)
            set({ client, transcript: messages })
            await setTranscript(transcript)
            onEvent?.('reset')
        } catch (error) {
            onEvent?.('error')
            set({ client: error })
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

        setCurrentTranscriptId(transcript.id)
        set({ transcript: messages })

        // find the transcript in history and update it
        const transcriptHistory = fetchTranscriptHistory()
        const transcriptJSONIndex = transcriptHistory.findIndex(({ id }) => id === transcript.id)
        if (transcriptJSONIndex !== -1) {
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
                const currentTranscriptId = fetchCurrentTranscriptId()
                const transcriptJSON =
                    transcriptHistory.find(({ id }) => id === currentTranscriptId) ||
                    transcriptHistory[transcriptHistory.length - 1]

                const transcript = Transcript.fromJSON(transcriptJSON)
                return transcript
            } catch {
                const newTranscript = new Transcript()
                void newTranscript.toJSON().then(transcriptJSON => saveTranscriptHistory([transcriptJSON]))
                return newTranscript
            }
        })()

        set({
            config,
            editor,
            onEvent,
            transcript: await initialTranscript.toChatPromise(),
            transcriptId: initialTranscript.id,
            transcriptHistory,
        })

        try {
            const client = await createClient({
                config: { ...config, customHeaders: window.context.xhrHeaders },
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
        const messages = await transcript.toChatPromise()

        try {
            const client = await createClient({
                config: { ...config, customHeaders: window.context.xhrHeaders },
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
        reset,
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
