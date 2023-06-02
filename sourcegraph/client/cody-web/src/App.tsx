import React, { useCallback, useEffect, useState } from 'react'

import { Client, Transcript, createClient } from '@sourcegraph/cody-shared/src/chat/client'
import { ChatMessage } from '@sourcegraph/cody-shared/src/chat/transcript/messages'
import type { Editor } from '@sourcegraph/cody-shared/src/editor'
import { CodySvg } from '@sourcegraph/cody-ui/src/utils/icons'
import { ErrorLike, isErrorLike } from '@sourcegraph/common'
import { Alert, LoadingSpinner } from '@sourcegraph/wildcard'

import { Chat } from './Chat'
import { Settings } from './settings/Settings'
import { useConfig } from './settings/useConfig'

import styles from './App.module.css'

/* eslint-disable @typescript-eslint/require-await */
const editor: Editor = {
    getActiveTextEditor() {
        return null
    },
    getActiveTextEditorSelection() {
        return null
    },
    getActiveTextEditorSelectionOrEntireFile() {
        return null
    },
    getActiveTextEditorVisibleContent() {
        return null
    },
    getWorkspaceRootPath() {
        return null
    },
    replaceSelection(_fileName, _selectedText, _replacement) {
        return Promise.resolve()
    },
    async showQuickPick(labels) {
        // TODO: Use a proper UI element
        return window.prompt(`Choose: ${labels.join(', ')}`, labels[0]) || undefined
    },
    async showWarningMessage(message) {
        console.warn(message)
    },
    async showInputBox(prompt?: string) {
        // TODO: Use a proper UI element
        return window.prompt(prompt || 'Enter here...') || undefined
    },
    didReceiveFixupText(_id: string, _text: string, _state: 'streaming' | 'complete'): Promise<void> {
        return Promise.resolve()
    },
}
/* eslint-enable @typescript-eslint/require-await */

export const App: React.FunctionComponent = () => {
    const [config, setConfig] = useConfig()
    const [messageInProgress, setMessageInProgress] = useState<ChatMessage | null>(null)
    const [transcript, setTranscript] = useState<ChatMessage[]>([])
    const [formInput, setFormInput] = useState('')
    const [inputHistory, setInputHistory] = useState<string[] | []>([])

    const [client, setClient] = useState<Client | ErrorLike>()
    useEffect(() => {
        setMessageInProgress(null)
        setTranscript([])
        createClient({
            config,
            setMessageInProgress,
            setTranscript: (transcript: Transcript) => setTranscript(transcript.toChat()),
            editor,
        }).then(setClient, setClient)
    }, [config])

    const onSubmit = useCallback(
        (text: string) => {
            if (client && !isErrorLike(client)) {
                // eslint-disable-next-line no-void
                void client.submitMessage(text)
            }
        },
        [client]
    )

    return (
        <div className={styles.container}>
            <header className={styles.header}>
                <h1>
                    <CodySvg /> Cody
                </h1>
                <Settings config={config} setConfig={setConfig} />
            </header>
            <main className={styles.main}>
                {!client ? (
                    <>
                        <LoadingSpinner />
                    </>
                ) : isErrorLike(client) ? (
                    <Alert className={styles.alert} variant="danger">
                        {client.message}
                    </Alert>
                ) : (
                    <>
                        <Chat
                            messageInProgress={messageInProgress}
                            transcript={transcript}
                            contextStatus={{ codebase: config.codebase }}
                            formInput={formInput}
                            setFormInput={setFormInput}
                            inputHistory={inputHistory}
                            setInputHistory={setInputHistory}
                            onSubmit={onSubmit}
                        />
                    </>
                )}
            </main>
        </div>
    )
}
