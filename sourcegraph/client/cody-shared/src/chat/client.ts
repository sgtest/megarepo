import { CodebaseContext } from '../codebase-context'
import { ConfigurationWithAccessToken } from '../configuration'
import { Editor } from '../editor'
import { PrefilledOptions, withPreselectedOptions } from '../editor/withPreselectedOptions'
import { SourcegraphEmbeddingsSearchClient } from '../embeddings/client'
import { SourcegraphIntentDetectorClient } from '../intent-detector/client'
import { SourcegraphBrowserCompletionsClient } from '../sourcegraph-api/completions/browserClient'
import { SourcegraphGraphQLAPIClient } from '../sourcegraph-api/graphql'
import { isError } from '../utils'

import { BotResponseMultiplexer } from './bot-response-multiplexer'
import { ChatClient } from './chat'
import { getPreamble } from './preamble'
import { getRecipe } from './recipes/browser-recipes'
import { RecipeID } from './recipes/recipe'
import { Transcript, TranscriptJSON } from './transcript'
import { ChatMessage } from './transcript/messages'
import { reformatBotMessage } from './viewHelpers'

export type { TranscriptJSON }
export { Transcript }

export interface ClientInit {
    config: Pick<
        ConfigurationWithAccessToken,
        'serverEndpoint' | 'codebase' | 'useContext' | 'accessToken' | 'customHeaders'
    >
    setMessageInProgress: (messageInProgress: ChatMessage | null) => void
    setTranscript: (transcript: Transcript) => void
    editor: Editor
    initialTranscript?: Transcript
}

export interface Client {
    readonly transcript: Transcript
    readonly isMessageInProgress: boolean
    submitMessage: (text: string) => Promise<void>
    executeRecipe: (
        recipeId: RecipeID,
        options?: {
            prefilledOptions?: PrefilledOptions
            humanChatInput?: string
        }
    ) => Promise<void>
    reset: () => void
    codebaseContext: CodebaseContext
}

export async function createClient({
    config,
    setMessageInProgress,
    setTranscript,
    editor,
    initialTranscript,
}: ClientInit): Promise<Client> {
    const fullConfig = { debugEnable: false, ...config }

    const completionsClient = new SourcegraphBrowserCompletionsClient(fullConfig)
    const chatClient = new ChatClient(completionsClient)

    const graphqlClient = new SourcegraphGraphQLAPIClient(fullConfig)

    const repoId = config.codebase ? await graphqlClient.getRepoIdIfEmbeddingExists(config.codebase) : null
    if (isError(repoId)) {
        throw new Error(
            `Cody could not access the '${config.codebase}' repository on your Sourcegraph instance. Details: ${repoId.message}`
        )
    }

    const embeddingsSearch = repoId ? new SourcegraphEmbeddingsSearchClient(graphqlClient, repoId, true) : null

    const codebaseContext = new CodebaseContext(config, config.codebase, embeddingsSearch, null, null)

    const intentDetector = new SourcegraphIntentDetectorClient(graphqlClient)

    const transcript = initialTranscript || new Transcript()

    let isMessageInProgress = false

    const sendTranscript = (): void => {
        if (isMessageInProgress) {
            const messages = transcript.toChat()
            setTranscript(transcript)
            setMessageInProgress(messages[messages.length - 1])
        } else {
            setTranscript(transcript)
            setMessageInProgress(null)
        }
    }

    async function executeRecipe(
        recipeId: RecipeID,
        options?: {
            prefilledOptions?: PrefilledOptions
            humanChatInput?: string
        }
    ): Promise<void> {
        const humanChatInput = options?.humanChatInput ?? ''
        const recipe = getRecipe(recipeId)
        if (!recipe) {
            return
        }

        const interaction = await recipe.getInteraction(humanChatInput, {
            editor: options?.prefilledOptions ? withPreselectedOptions(editor, options.prefilledOptions) : editor,
            intentDetector,
            codebaseContext,
            responseMultiplexer: new BotResponseMultiplexer(),
            firstInteraction: transcript.isEmpty,
        })
        if (!interaction) {
            return
        }
        isMessageInProgress = true
        transcript.addInteraction(interaction)

        sendTranscript()

        const { prompt, contextFiles } = await transcript.getPromptForLastInteraction(getPreamble(config.codebase))
        transcript.setUsedContextFilesForLastInteraction(contextFiles)

        const responsePrefix = interaction.getAssistantMessage().prefix ?? ''
        let rawText = ''
        chatClient.chat(prompt, {
            onChange(_rawText) {
                rawText = _rawText

                const text = reformatBotMessage(rawText, responsePrefix)
                transcript.addAssistantResponse(text)

                sendTranscript()
            },
            onComplete() {
                isMessageInProgress = false

                const text = reformatBotMessage(rawText, responsePrefix)
                transcript.addAssistantResponse(text)
                sendTranscript()
            },
            onError(error) {
                // Display error message as assistant response
                transcript.addErrorAsAssistantResponse(error)
                isMessageInProgress = false
                sendTranscript()
                console.error(`Completion request failed: ${error}`)
            },
        })
    }

    return {
        get transcript() {
            return transcript
        },
        get isMessageInProgress() {
            return isMessageInProgress
        },
        submitMessage(text: string) {
            return executeRecipe('chat-question', { humanChatInput: text })
        },
        executeRecipe,
        reset() {
            isMessageInProgress = false
            transcript.reset()
            sendTranscript()
        },
        codebaseContext,
    }
}
