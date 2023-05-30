import React, { useCallback, useMemo, useState } from 'react'

import classNames from 'classnames'

import { ChatContextStatus } from '@sourcegraph/cody-shared/src/chat/context'
import { ChatMessage } from '@sourcegraph/cody-shared/src/chat/transcript/messages'
import { isDefined } from '@sourcegraph/common'

import { FileLinkProps } from './chat/ContextFiles'
import { ChatInputContext } from './chat/inputContext/ChatInputContext'
import { Transcript } from './chat/Transcript'
import { TranscriptItemClassNames } from './chat/TranscriptItem'

import styles from './Chat.module.css'

interface ChatProps extends ChatClassNames {
    transcript: ChatMessage[]
    messageInProgress: ChatMessage | null
    messageBeingEdited: boolean
    setMessageBeingEdited: (input: boolean) => void
    contextStatus?: ChatContextStatus | null
    formInput: string
    setFormInput: (input: string) => void
    inputHistory: string[]
    setInputHistory: (history: string[]) => void
    onSubmit: (text: string, submitType: 'user' | 'suggestion') => void
    textAreaComponent: React.FunctionComponent<ChatUITextAreaProps>
    submitButtonComponent: React.FunctionComponent<ChatUISubmitButtonProps>
    suggestionButtonComponent?: React.FunctionComponent<ChatUISuggestionButtonProps>
    fileLinkComponent: React.FunctionComponent<FileLinkProps>
    afterTips?: string
    className?: string
    EditButtonContainer?: React.FunctionComponent<EditButtonProps>
    editButtonOnSubmit?: (text: string) => void
    FeedbackButtonsContainer?: React.FunctionComponent<FeedbackButtonsProps>
    feedbackButtonsOnSubmit?: (text: string) => void
    copyButtonOnSubmit?: CopyButtonProps['copyButtonOnSubmit']
    suggestions?: string[]
    setSuggestions?: (suggestions: undefined | []) => void
    needsEmailVerification?: boolean
    needsEmailVerificationNotice?: React.FunctionComponent
}

interface ChatClassNames extends TranscriptItemClassNames {
    inputRowClassName?: string
    chatInputContextClassName?: string
    chatInputClassName?: string
}

export interface ChatUITextAreaProps {
    className: string
    rows: number
    autoFocus: boolean
    value: string
    required: boolean
    disabled?: boolean
    onInput: React.FormEventHandler<HTMLElement>
    onKeyDown?: (event: React.KeyboardEvent<HTMLElement>, caretPosition: number | null) => void
}

export interface ChatUISubmitButtonProps {
    className: string
    disabled: boolean
    onClick: (event: React.MouseEvent<HTMLButtonElement>) => void
}

export interface ChatUISuggestionButtonProps {
    suggestion: string
    onClick: (event: React.MouseEvent<HTMLButtonElement>) => void
}

export interface EditButtonProps {
    className: string
    disabled?: boolean
    messageBeingEdited: boolean
    setMessageBeingEdited: (input: boolean) => void
}

export interface FeedbackButtonsProps {
    className: string
    disabled?: boolean
    feedbackButtonsOnSubmit: (text: string) => void
}

// TODO: Rename to CodeBlockActionsProps
export interface CopyButtonProps {
    copyButtonOnSubmit: (text: string, insert?: boolean) => void
}
/**
 * The Cody chat interface, with a transcript of all messages and a message form.
 */
export const Chat: React.FunctionComponent<ChatProps> = ({
    messageInProgress,
    messageBeingEdited,
    setMessageBeingEdited,
    transcript,
    contextStatus,
    formInput,
    setFormInput,
    inputHistory,
    setInputHistory,
    onSubmit,
    textAreaComponent: TextArea,
    submitButtonComponent: SubmitButton,
    suggestionButtonComponent: SuggestionButton,
    fileLinkComponent,
    afterTips,
    className,
    codeBlocksCopyButtonClassName,
    codeBlocksInsertButtonClassName,
    transcriptItemClassName,
    humanTranscriptItemClassName,
    transcriptItemParticipantClassName,
    transcriptActionClassName,
    inputRowClassName,
    chatInputContextClassName,
    chatInputClassName,
    EditButtonContainer,
    editButtonOnSubmit,
    FeedbackButtonsContainer,
    feedbackButtonsOnSubmit,
    copyButtonOnSubmit,
    suggestions,
    setSuggestions,
    needsEmailVerification = false,
    needsEmailVerificationNotice: NeedsEmailVerificationNotice,
}) => {
    const [inputRows, setInputRows] = useState(5)
    const [historyIndex, setHistoryIndex] = useState(inputHistory.length)

    const inputHandler = useCallback(
        (inputValue: string): void => {
            const rowsCount = inputValue.match(/\n/g)?.length
            if (rowsCount) {
                setInputRows(rowsCount < 5 ? 5 : rowsCount > 25 ? 25 : rowsCount)
            } else {
                setInputRows(5)
            }
            setFormInput(inputValue)
            if (inputValue !== inputHistory[historyIndex]) {
                setHistoryIndex(inputHistory.length)
            }
        },
        [historyIndex, inputHistory, setFormInput]
    )

    const submitInput = useCallback(
        (input: string, submitType: 'user' | 'suggestion'): void => {
            if (messageInProgress) {
                return
            }

            onSubmit(input, submitType)
            setSuggestions?.(undefined)
            setHistoryIndex(inputHistory.length + 1)
            setInputHistory([...inputHistory, input])
        },
        [inputHistory, messageInProgress, onSubmit, setInputHistory, setSuggestions]
    )

    const onChatSubmit = useCallback((): void => {
        // Submit chat only when input is not empty and not in progress
        if (formInput.trim() && !messageInProgress) {
            setInputRows(5)
            setFormInput('')
            submitInput(formInput, 'user')
        }
    }, [formInput, messageInProgress, setFormInput, submitInput])

    const onChatKeyDown = useCallback(
        (event: React.KeyboardEvent<HTMLElement>, caretPosition: number | null): void => {
            // Submit input on Enter press (without shift) and
            // trim the formInput to make sure input value is not empty.
            if (
                event.key === 'Enter' &&
                !event.shiftKey &&
                !event.nativeEvent.isComposing &&
                formInput &&
                formInput.trim()
            ) {
                event.preventDefault()
                event.stopPropagation()
                setMessageBeingEdited(false)
                onChatSubmit()
            }

            // Loop through input history on up arrow press
            if (!inputHistory.length) {
                return
            }

            if (formInput === inputHistory[historyIndex] || !formInput) {
                if (event.key === 'ArrowUp' && caretPosition === 0) {
                    const newIndex = historyIndex - 1 < 0 ? inputHistory.length - 1 : historyIndex - 1
                    setHistoryIndex(newIndex)
                    setFormInput(inputHistory[newIndex])
                } else if (event.key === 'ArrowDown' && caretPosition === formInput.length) {
                    const newIndex = historyIndex + 1 >= inputHistory.length ? 0 : historyIndex + 1
                    setHistoryIndex(newIndex)
                    setFormInput(inputHistory[newIndex])
                }
            }
        },
        [inputHistory, historyIndex, setFormInput, onChatSubmit, formInput, setMessageBeingEdited]
    )

    const transcriptWithWelcome = useMemo<ChatMessage[]>(
        () => [
            {
                speaker: 'assistant',
                displayText: welcomeText(afterTips),
            },
            ...transcript,
        ],
        [afterTips, transcript]
    )

    return (
        <div className={classNames(className, styles.innerContainer)}>
            {needsEmailVerification && NeedsEmailVerificationNotice ? (
                <div className="flex-1">
                    <NeedsEmailVerificationNotice />
                </div>
            ) : (
                <Transcript
                    transcript={transcriptWithWelcome}
                    messageInProgress={messageInProgress}
                    messageBeingEdited={messageBeingEdited}
                    setMessageBeingEdited={setMessageBeingEdited}
                    fileLinkComponent={fileLinkComponent}
                    codeBlocksCopyButtonClassName={codeBlocksCopyButtonClassName}
                    codeBlocksInsertButtonClassName={codeBlocksInsertButtonClassName}
                    transcriptItemClassName={transcriptItemClassName}
                    humanTranscriptItemClassName={humanTranscriptItemClassName}
                    transcriptItemParticipantClassName={transcriptItemParticipantClassName}
                    transcriptActionClassName={transcriptActionClassName}
                    className={styles.transcriptContainer}
                    textAreaComponent={TextArea}
                    EditButtonContainer={EditButtonContainer}
                    editButtonOnSubmit={editButtonOnSubmit}
                    FeedbackButtonsContainer={FeedbackButtonsContainer}
                    feedbackButtonsOnSubmit={feedbackButtonsOnSubmit}
                    copyButtonOnSubmit={copyButtonOnSubmit}
                    submitButtonComponent={SubmitButton}
                    chatInputClassName={chatInputClassName}
                />
            )}

            <form className={classNames(styles.inputRow, inputRowClassName)}>
                {suggestions !== undefined && suggestions.length !== 0 && SuggestionButton ? (
                    <div className={styles.suggestions}>
                        {suggestions.map((suggestion: string) =>
                            suggestion.trim().length > 0 ? (
                                <SuggestionButton
                                    key={suggestion}
                                    suggestion={suggestion}
                                    onClick={() => submitInput(suggestion, 'suggestion')}
                                />
                            ) : null
                        )}
                    </div>
                ) : null}
                <div className={styles.textAreaContainer}>
                    <TextArea
                        className={classNames(styles.chatInput, chatInputClassName)}
                        rows={inputRows}
                        value={formInput}
                        autoFocus={true}
                        required={true}
                        disabled={needsEmailVerification}
                        onInput={({ target }) => {
                            const { value } = target as HTMLInputElement
                            inputHandler(value)
                        }}
                        onKeyDown={onChatKeyDown}
                    />
                    <SubmitButton
                        className={styles.submitButton}
                        onClick={onChatSubmit}
                        disabled={!!messageInProgress || needsEmailVerification}
                    />
                </div>
                {contextStatus && (
                    <ChatInputContext contextStatus={contextStatus} className={chatInputContextClassName} />
                )}
            </form>
        </div>
    )
}

function welcomeText(afterTips?: string): string {
    return [
        "Hello! I'm Cody. I can write code and answer questions for you. See [Cody documentation](https://docs.sourcegraph.com/cody) for help and tips.",
        afterTips,
    ]
        .filter(isDefined)
        .join('\n\n')
}
