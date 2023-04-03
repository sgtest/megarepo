import React from 'react'

import { Chat as ChatUI, ChatUISubmitButtonProps, ChatUITextAreaProps } from '@sourcegraph/cody-ui/src/Chat'
import { FileLinkProps } from '@sourcegraph/cody-ui/src/chat/ContextFiles'
import { Terms } from '@sourcegraph/cody-ui/src/Terms'
import { SubmitSvg } from '@sourcegraph/cody-ui/src/utils/icons'

import styles from './Chat.module.css'

export const Chat: React.FunctionComponent<
    Omit<
        React.ComponentPropsWithoutRef<typeof ChatUI>,
        'textAreaComponent' | 'submitButtonComponent' | 'fileLinkComponent'
    >
> = ({ messageInProgress, transcript, formInput, setFormInput, inputHistory, setInputHistory, onSubmit }) => (
    <ChatUI
        messageInProgress={messageInProgress}
        transcript={transcript}
        formInput={formInput}
        setFormInput={setFormInput}
        inputHistory={inputHistory}
        setInputHistory={setInputHistory}
        onSubmit={onSubmit}
        textAreaComponent={TextArea}
        submitButtonComponent={SubmitButton}
        fileLinkComponent={FileLink}
        afterTips={
            <details className={styles.terms}>
                <summary>Terms</summary>
                <Terms />
            </details>
        }
        bubbleContentClassName={styles.bubbleContent}
        humanBubbleContentClassName={styles.humanBubbleContent}
        botBubbleContentClassName={styles.botBubbleContent}
        bubbleFooterClassName={styles.bubbleFooter}
        bubbleLoaderDotClassName={styles.bubbleLoaderDot}
        inputRowClassName={styles.inputRow}
        chatInputClassName={styles.chatInput}
    />
)

const TextArea: React.FunctionComponent<ChatUITextAreaProps> = ({
    className,
    rows,
    autoFocus,
    value,
    required,
    onInput,
    onKeyDown,
}) => (
    <textarea
        className={className}
        rows={rows}
        value={value}
        autoFocus={autoFocus}
        required={required}
        onInput={onInput}
        onKeyDown={onKeyDown}
    />
)

const SubmitButton: React.FunctionComponent<ChatUISubmitButtonProps> = ({ className, disabled, onClick }) => (
    <button className={className} type="submit" disabled={disabled} onClick={onClick}>
        <SubmitSvg />
    </button>
)

const FileLink: React.FunctionComponent<FileLinkProps> = ({ path }) => <>{path}</>
