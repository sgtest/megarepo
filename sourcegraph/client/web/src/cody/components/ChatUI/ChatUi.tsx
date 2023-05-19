import { useCallback, useEffect, useRef, useState } from 'react'

import { mdiClose, mdiSend, mdiArrowDown, mdiPencil, mdiThumbUp, mdiThumbDown, mdiCheck } from '@mdi/js'
import classNames from 'classnames'
import useResizeObserver from 'use-resize-observer'

import {
    Chat,
    ChatUISubmitButtonProps,
    ChatUITextAreaProps,
    EditButtonProps,
    FeedbackButtonsProps,
} from '@sourcegraph/cody-ui/src/Chat'
import { FileLinkProps } from '@sourcegraph/cody-ui/src/chat/ContextFiles'
import { CODY_TERMS_MARKDOWN } from '@sourcegraph/cody-ui/src/terms'
import { Button, Icon, TextArea, Link, Tooltip, Alert, Text, H2 } from '@sourcegraph/wildcard'

import { eventLogger } from '../../../tracking/eventLogger'
import { CodyPageIcon } from '../../chat/CodyPageIcon'
import { useChatStoreState } from '../../stores/chat'
import { useCodySidebarStore } from '../../stores/sidebar'
import { useIsCodyEnabled } from '../../useIsCodyEnabled'

import styles from './ChatUi.module.scss'

export const SCROLL_THRESHOLD = 100

const onFeedbackSubmit = (feedback: string): void => eventLogger.log(`web:cody:feedbackSubmit:${feedback}`)

export const ChatUI = (): JSX.Element => {
    const {
        submitMessage,
        editMessage,
        messageInProgress,
        transcript,
        getChatContext,
        transcriptId,
        transcriptHistory,
    } = useChatStoreState()
    const { needsEmailVerification } = useIsCodyEnabled()

    const [formInput, setFormInput] = useState('')
    const [inputHistory, setInputHistory] = useState<string[] | []>(() =>
        transcriptHistory
            .flatMap(entry => entry.interactions)
            .sort((entryA, entryB) => +new Date(entryA.timestamp) - +new Date(entryB.timestamp))
            .filter(interaction => interaction.humanMessage.displayText !== undefined)
            .map(interaction => interaction.humanMessage.displayText!)
    )
    const [messageBeingEdited, setMessageBeingEdited] = useState<boolean>(false)

    return (
        <Chat
            key={transcriptId}
            messageInProgress={messageInProgress}
            messageBeingEdited={messageBeingEdited}
            setMessageBeingEdited={setMessageBeingEdited}
            transcript={transcript}
            formInput={formInput}
            setFormInput={setFormInput}
            inputHistory={inputHistory}
            setInputHistory={setInputHistory}
            onSubmit={submitMessage}
            contextStatus={getChatContext()}
            submitButtonComponent={SubmitButton}
            fileLinkComponent={FileLink}
            className={styles.container}
            afterTips={CODY_TERMS_MARKDOWN}
            transcriptItemClassName={styles.transcriptItem}
            humanTranscriptItemClassName={styles.humanTranscriptItem}
            transcriptItemParticipantClassName="text-muted"
            inputRowClassName={styles.inputRow}
            chatInputClassName={styles.chatInput}
            EditButtonContainer={EditButton}
            editButtonOnSubmit={editMessage}
            textAreaComponent={AutoResizableTextArea}
            codeBlocksCopyButtonClassName={styles.codeBlocksCopyButton}
            transcriptActionClassName={styles.transcriptAction}
            FeedbackButtonsContainer={FeedbackButtons}
            feedbackButtonsOnSubmit={onFeedbackSubmit}
            needsEmailVerification={needsEmailVerification}
            needsEmailVerificationNotice={NeedsEmailVerificationNotice}
        />
    )
}

export const ScrollDownButton = ({ onClick }: { onClick: () => void }): JSX.Element => (
    <div className={styles.scrollButtonWrapper}>
        <Button className={styles.scrollButton} onClick={onClick}>
            <Icon aria-label="Scroll down" svgPath={mdiArrowDown} />
        </Button>
    </div>
)

export const EditButton: React.FunctionComponent<EditButtonProps> = ({
    className,
    messageBeingEdited,
    setMessageBeingEdited,
}) => (
    <div className={className}>
        <button
            className={classNames(className, styles.editButton)}
            type="button"
            onClick={() => setMessageBeingEdited(!messageBeingEdited)}
        >
            {messageBeingEdited ? (
                <Icon aria-label="Close" svgPath={mdiClose} />
            ) : (
                <Icon aria-label="Edit" svgPath={mdiPencil} />
            )}
        </button>
    </div>
)

const FeedbackButtons: React.FunctionComponent<FeedbackButtonsProps> = ({ feedbackButtonsOnSubmit }) => {
    const [feedbackSubmitted, setFeedbackSubmitted] = useState(false)

    const onFeedbackBtnSubmit = useCallback(
        (text: string) => {
            feedbackButtonsOnSubmit(text)
            setFeedbackSubmitted(true)
        },
        [feedbackButtonsOnSubmit]
    )

    return (
        <div className={classNames('d-flex', styles.feedbackButtonsWrapper)}>
            {feedbackSubmitted ? (
                <Button title="Feedback submitted." disabled={true} className="ml-1 p-1">
                    <Icon aria-label="Feedback submitted" svgPath={mdiCheck} />
                </Button>
            ) : (
                <>
                    <Button
                        title="Thumbs up"
                        className="ml-1 p-1"
                        type="button"
                        onClick={() => onFeedbackBtnSubmit('positive')}
                    >
                        <Icon aria-label="Thumbs up" svgPath={mdiThumbUp} />
                    </Button>
                    <Button
                        title="Thumbs up"
                        className="ml-1 p-1"
                        type="button"
                        onClick={() => onFeedbackBtnSubmit('negative')}
                    >
                        <Icon aria-label="Thumbs down" svgPath={mdiThumbDown} />
                    </Button>
                </>
            )}
        </div>
    )
}

export const SubmitButton: React.FunctionComponent<ChatUISubmitButtonProps> = ({ className, disabled, onClick }) => (
    <button className={classNames(className, styles.submitButton)} type="submit" disabled={disabled} onClick={onClick}>
        <Icon aria-label="Submit" svgPath={mdiSend} />
    </button>
)

export const FileLink: React.FunctionComponent<FileLinkProps> = ({ path, repoName, revision }) =>
    repoName ? <Link to={`/${repoName}${revision ? `@${revision}` : ''}/-/blob/${path}`}>{path}</Link> : <>{path}</>

interface AutoResizableTextAreaProps extends ChatUITextAreaProps {}

export const AutoResizableTextArea: React.FC<AutoResizableTextAreaProps> = ({
    value,
    onInput,
    onKeyDown,
    className,
    disabled = false,
}) => {
    const { inputNeedsFocus, setFocusProvided } = useCodySidebarStore()
    const { needsEmailVerification } = useIsCodyEnabled()
    const textAreaRef = useRef<HTMLTextAreaElement>(null)
    const { width = 0 } = useResizeObserver({ ref: textAreaRef })

    const adjustTextAreaHeight = useCallback((): void => {
        if (textAreaRef.current) {
            textAreaRef.current.style.height = '0px'
            const scrollHeight = textAreaRef.current.scrollHeight
            textAreaRef.current.style.height = `${scrollHeight}px`

            // Hide scroll if the textArea isn't overflowing.
            textAreaRef.current.style.overflowY = scrollHeight < 200 ? 'hidden' : 'auto'
        }
    }, [])

    const handleChange = (): void => {
        adjustTextAreaHeight()
    }

    useEffect(() => {
        if (inputNeedsFocus && textAreaRef.current) {
            textAreaRef.current.focus()
            setFocusProvided()
        }
    }, [inputNeedsFocus, setFocusProvided])

    useEffect(() => {
        adjustTextAreaHeight()
    }, [adjustTextAreaHeight, value, width])

    const handleKeyDown = (event: React.KeyboardEvent<HTMLElement>): void => {
        if (onKeyDown) {
            onKeyDown(event, textAreaRef.current?.selectionStart ?? null)
        }
    }

    return (
        <Tooltip content={needsEmailVerification ? 'Verify your email to use Cody.' : ''}>
            <TextArea
                ref={textAreaRef}
                className={className}
                value={value}
                onChange={handleChange}
                rows={1}
                autoFocus={false}
                required={true}
                onKeyDown={handleKeyDown}
                onInput={onInput}
                disabled={disabled}
            />
        </Tooltip>
    )
}

const NeedsEmailVerificationNotice: React.FunctionComponent = () => (
    <div className="p-3">
        <H2 className={classNames('d-flex gap-1 align-items-center mb-3', styles.codyMessageHeader)}>
            <CodyPageIcon /> Cody
        </H2>
        <Alert variant="warning">
            <Text className="mb-0">Verify email</Text>
            <Text className="mb-0">
                Using Cody requires a verified email.{' '}
                <Link to={`${window.context.currentUser?.settingsURL}/emails`} target="_blank" rel="noreferrer">
                    Resend email verification
                </Link>
                .
            </Text>
        </Alert>
    </div>
)
