import { spawnSync } from 'child_process'
import path from 'path'

import * as vscode from 'vscode'

import { BotResponseMultiplexer } from '@sourcegraph/cody-shared/src/chat/bot-response-multiplexer'
import { ChatClient } from '@sourcegraph/cody-shared/src/chat/chat'
import { escapeCodyMarkdown } from '@sourcegraph/cody-shared/src/chat/markdown'
import { getPreamble } from '@sourcegraph/cody-shared/src/chat/preamble'
import { getRecipe } from '@sourcegraph/cody-shared/src/chat/recipes/vscode-recipes'
import { Transcript } from '@sourcegraph/cody-shared/src/chat/transcript'
import { ChatMessage, ChatHistory } from '@sourcegraph/cody-shared/src/chat/transcript/messages'
import { reformatBotMessage } from '@sourcegraph/cody-shared/src/chat/viewHelpers'
import { CodebaseContext } from '@sourcegraph/cody-shared/src/codebase-context'
import { ConfigurationWithAccessToken } from '@sourcegraph/cody-shared/src/configuration'
import { Editor } from '@sourcegraph/cody-shared/src/editor'
import { SourcegraphEmbeddingsSearchClient } from '@sourcegraph/cody-shared/src/embeddings/client'
import { highlightTokens } from '@sourcegraph/cody-shared/src/hallucinations-detector'
import { IntentDetector } from '@sourcegraph/cody-shared/src/intent-detector'
import { Message } from '@sourcegraph/cody-shared/src/sourcegraph-api'
import { SourcegraphGraphQLAPIClient } from '@sourcegraph/cody-shared/src/sourcegraph-api/graphql'
import { isError } from '@sourcegraph/cody-shared/src/utils'

import { View } from '../../webviews/NavBar'
import { LocalStorage } from '../command/LocalStorageProvider'
import { getFullConfig, updateConfiguration } from '../configuration'
import { logEvent } from '../event-logger'
import { LocalKeywordContextFetcher } from '../keyword-context/local-keyword-context-fetcher'
import { CODY_ACCESS_TOKEN_SECRET, SecretStorage } from '../secret-storage'
import { TestSupport } from '../test-support'

import { ConfigurationSubsetForWebview, DOTCOM_URL, ExtensionMessage, WebviewMessage } from './protocol'

export async function isValidLogin(
    config: Pick<ConfigurationWithAccessToken, 'serverEndpoint' | 'accessToken' | 'customHeaders'>
): Promise<boolean> {
    const client = new SourcegraphGraphQLAPIClient(config)
    const userId = await client.getCurrentUserId()
    return !isError(userId)
}

type Config = Pick<
    ConfigurationWithAccessToken,
    | 'codebase'
    | 'serverEndpoint'
    | 'debug'
    | 'customHeaders'
    | 'accessToken'
    | 'useContext'
    | 'experimentalChatPredictions'
>

export class ChatViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
    private isMessageInProgress = false
    private cancelCompletionCallback: (() => void) | null = null
    private webview?: Omit<vscode.Webview, 'postMessage'> & {
        postMessage(message: ExtensionMessage): Thenable<boolean>
    }

    private currentChatID = ''
    private inputHistory: string[] = []
    private chatHistory: ChatHistory = {}

    private transcript: Transcript = new Transcript()

    // Allows recipes to hook up subscribers to process sub-streams of bot output
    private multiplexer: BotResponseMultiplexer = new BotResponseMultiplexer()

    private configurationChangeEvent = new vscode.EventEmitter<void>()

    private disposables: vscode.Disposable[] = []

    // Codebase-context-related state
    private currentWorkspaceRoot: string

    constructor(
        private extensionPath: string,
        private config: Omit<Config, 'codebase'>, // should use codebaseContext.getCodebase() rather than config.codebase
        private chat: ChatClient,
        private intentDetector: IntentDetector,
        private codebaseContext: CodebaseContext,
        private editor: Editor,
        private secretStorage: SecretStorage,
        private localStorage: LocalStorage,
        private rgPath: string
    ) {
        if (TestSupport.instance) {
            TestSupport.instance.chatViewProvider.set(this)
        }
        // chat id is used to identify chat session
        this.createNewChatID()

        this.disposables.push(this.configurationChangeEvent)

        // listen for vscode active editor change event
        this.currentWorkspaceRoot = ''
        this.disposables.push(
            vscode.window.onDidChangeActiveTextEditor(async () => {
                await this.updateCodebaseContext()
            }),
            vscode.workspace.onDidChangeConfiguration(async () => {
                this.config = await getFullConfig(this.secretStorage)
                const newCodebaseContext = await getCodebaseContext(this.config, this.rgPath, this.editor)
                if (newCodebaseContext) {
                    this.codebaseContext = newCodebaseContext
                }
            })
        )
    }

    public onConfigurationChange(newConfig: Config): void {
        this.config = newConfig
        this.configurationChangeEvent.fire()
    }

    public async clearAndRestartSession(): Promise<void> {
        await this.saveTranscriptToChatHistory()
        this.createNewChatID()
        this.cancelCompletion()
        this.isMessageInProgress = false
        this.transcript.reset()
        this.sendSuggestions([])
        this.sendTranscript()
        this.sendChatHistory()
    }

    public async clearHistory(): Promise<void> {
        this.chatHistory = {}
        this.inputHistory = []
        await this.localStorage.removeChatHistory()
    }

    /**
     * Restores a session from a chatID
     * We delete the loaded session from our in-memory chatHistory (to hide it from the history view)
     * but don't modify the localStorage as no data changes when a session is restored
     */
    public async restoreSession(chatID: string): Promise<void> {
        await this.saveTranscriptToChatHistory()
        this.cancelCompletion()
        this.currentChatID = chatID
        this.transcript = Transcript.fromJSON(this.chatHistory[chatID])
        delete this.chatHistory[chatID]
        this.sendTranscript()
        this.sendChatHistory()
    }

    private async onDidReceiveMessage(message: WebviewMessage): Promise<void> {
        switch (message.command) {
            case 'initialized':
                this.loadChatHistory()
                this.publishContextStatus()
                this.publishConfig()
                this.sendTranscript()
                this.sendChatHistory()
                break
            case 'submit':
                await this.onHumanMessageSubmitted(message.text, message.submitType)
                break
            case 'edit':
                this.transcript.removeLastInteraction()
                await this.onHumanMessageSubmitted(message.text, 'user')
                break
            case 'executeRecipe':
                await this.executeRecipe(message.recipe)
                break
            case 'settings': {
                const isValid = await isValidLogin({
                    serverEndpoint: message.serverEndpoint,
                    accessToken: message.accessToken,
                    customHeaders: this.config.customHeaders,
                })
                // activate when user has valid login
                await vscode.commands.executeCommand('setContext', 'cody.activated', isValid)
                if (isValid) {
                    await updateConfiguration('serverEndpoint', message.serverEndpoint)
                    await this.secretStorage.store(CODY_ACCESS_TOKEN_SECRET, message.accessToken)
                    this.sendEvent('auth', 'login')
                }
                void this.webview?.postMessage({ type: 'login', isValid })
                break
            }
            case 'event':
                this.sendEvent(message.event, message.value)
                break
            case 'removeToken':
                await this.secretStorage.delete(CODY_ACCESS_TOKEN_SECRET)
                this.sendEvent('token', 'Delete')
                break
            case 'removeHistory':
                await this.clearHistory()
                break
            case 'restoreHistory':
                await this.restoreSession(message.chatID)
                break
            case 'links':
                void this.openExternalLinks(message.value)
                break
            case 'openFile': {
                const rootPath = this.editor.getWorkspaceRootPath()
                if (!rootPath) {
                    this.sendErrorToWebview('Failed to open file: missing rootPath')
                    return
                }
                try {
                    // This opens the file in the active column.
                    const uri = vscode.Uri.file(path.join(rootPath, message.filePath))
                    const doc = await vscode.workspace.openTextDocument(uri)
                    await vscode.window.showTextDocument(doc)
                } catch {
                    // Try to open the file in the sourcegraph view
                    const sourcegraphSearchURL = new URL(
                        `/search?q=context:global+file:${message.filePath}`,
                        this.config.serverEndpoint
                    ).href
                    void this.openExternalLinks(sourcegraphSearchURL)
                }
                break
            }
            default:
                this.sendErrorToWebview('Invalid request type from Webview')
        }
    }

    private createNewChatID(): void {
        this.currentChatID = new Date(Date.now()).toUTCString()
    }

    private sendPrompt(promptMessages: Message[], responsePrefix = ''): void {
        this.cancelCompletion()

        let text = ''

        this.multiplexer.sub(BotResponseMultiplexer.DEFAULT_TOPIC, {
            onResponse: (content: string) => {
                text += content
                this.transcript.addAssistantResponse(reformatBotMessage(escapeCodyMarkdown(text), responsePrefix))
                this.sendTranscript()
                return Promise.resolve()
            },
            onTurnComplete: async () => {
                const lastInteraction = this.transcript.getLastInteraction()
                if (lastInteraction) {
                    const { text, displayText } = lastInteraction.getAssistantMessage()
                    const { text: highlightedDisplayText } = await highlightTokens(displayText || '', filesExist)
                    this.transcript.addAssistantResponse(text || '', highlightedDisplayText)
                }
                void this.onCompletionEnd()
            },
        })

        let textConsumed = 0

        this.cancelCompletionCallback = this.chat.chat(promptMessages, {
            onChange: text => {
                // TODO(dpc): The multiplexer can handle incremental text. Change chat to provide incremental text.
                text = text.slice(textConsumed)
                textConsumed += text.length
                void this.multiplexer.publish(text)
            },
            onComplete: () => {
                void this.multiplexer.notifyTurnComplete()
            },
            onError: (err, statusCode) => {
                // Display error message as assistant response
                this.transcript.addErrorAsAssistantResponse(
                    `<div class="cody-chat-error"><span>Request failed: </span>${err}</div>`
                )
                // Log users out on unauth error
                if (statusCode && statusCode >= 400 && statusCode <= 410) {
                    void this.sendLogin(false)
                    void this.clearAndRestartSession()
                }
                this.onCompletionEnd()
                console.error(`Completion request failed: ${err}`)
            },
        })
    }

    private cancelCompletion(): void {
        this.cancelCompletionCallback?.()
        this.cancelCompletionCallback = null
    }

    private onCompletionEnd(): void {
        this.isMessageInProgress = false
        this.cancelCompletionCallback = null
        this.sendTranscript()
        void this.saveTranscriptToChatHistory()
    }

    private async onHumanMessageSubmitted(text: string, submitType: 'user' | 'suggestion'): Promise<void> {
        if (submitType === 'suggestion') {
            logEvent('CodyVSCodeExtension:chatPredictions:used')
        }
        this.inputHistory.push(text)
        if (this.config.experimentalChatPredictions) {
            void this.runRecipeForSuggestion('next-questions', text)
        }
        await this.executeChatCommands(text)
    }

    private async executeChatCommands(text: string): Promise<void> {
        const command = text.split(' ')[0]
        switch (command) {
            case '/reset':
            case '/r':
                await this.clearAndRestartSession()
                break
            case '/search':
            case '/s':
                await this.executeRecipe('context-search', text)
                break
            default:
                return this.executeRecipe('chat-question', text)
        }
    }

    private async updateCodebaseContext(): Promise<void> {
        if (!this.editor.getActiveTextEditor() && vscode.window.visibleTextEditors.length !== 0) {
            // these are ephemeral
            return
        }
        const workspaceRoot = this.editor.getWorkspaceRootPath()
        if (!workspaceRoot || workspaceRoot === '' || workspaceRoot === this.currentWorkspaceRoot) {
            return
        }
        this.currentWorkspaceRoot = workspaceRoot

        const codebaseContext = await getCodebaseContext(this.config, this.rgPath, this.editor)
        if (!codebaseContext) {
            return
        }
        // after await, check we're still hitting the same workspace root
        if (this.currentWorkspaceRoot !== workspaceRoot) {
            return
        }

        this.codebaseContext = codebaseContext
        this.publishContextStatus()
    }

    public async executeRecipe(recipeId: string, humanChatInput: string = ''): Promise<void> {
        if (this.isMessageInProgress) {
            this.sendErrorToWebview('Cannot execute multiple recipes. Please wait for the current recipe to finish.')
        }

        const recipe = getRecipe(recipeId)
        if (!recipe) {
            return
        }

        // Create a new multiplexer to drop any old subscribers
        this.multiplexer = new BotResponseMultiplexer()

        const interaction = await recipe.getInteraction(humanChatInput, {
            editor: this.editor,
            intentDetector: this.intentDetector,
            codebaseContext: this.codebaseContext,
            responseMultiplexer: this.multiplexer,
        })
        if (!interaction) {
            return
        }
        this.isMessageInProgress = true
        this.transcript.addInteraction(interaction)

        this.showTab('chat')

        // Check whether or not to connect to LLM backend for responses
        // Ex: performing fuzzy / context-search does not require responses from LLM backend
        const prompt = await this.transcript.toPrompt(getPreamble(this.codebaseContext.getCodebase()))

        switch (recipeId) {
            case 'context-search':
                this.onCompletionEnd()
                break
            default:
                this.sendTranscript()
                this.sendPrompt(prompt, interaction.getAssistantMessage().prefix ?? '')
                await this.saveTranscriptToChatHistory()
        }

        logEvent(`CodyVSCodeExtension:recipe:${recipe.id}:executed`)
    }

    private async runRecipeForSuggestion(recipeId: string, humanChatInput: string = ''): Promise<void> {
        const recipe = getRecipe(recipeId)
        if (!recipe) {
            return
        }

        const multiplexer = new BotResponseMultiplexer()
        const transcript = Transcript.fromJSON(await this.transcript.toJSON())

        const interaction = await recipe.getInteraction(humanChatInput, {
            editor: this.editor,
            intentDetector: this.intentDetector,
            codebaseContext: this.codebaseContext,
            responseMultiplexer: multiplexer,
        })
        if (!interaction) {
            return
        }
        transcript.addInteraction(interaction)

        const prompt = await transcript.toPrompt(getPreamble(this.codebaseContext.getCodebase()))

        logEvent(`CodyVSCodeExtension:recipe:${recipe.id}:executed`)

        let text = ''
        multiplexer.sub(BotResponseMultiplexer.DEFAULT_TOPIC, {
            onResponse: (content: string) => {
                text += content
                return Promise.resolve()
            },
            onTurnComplete: () => {
                const suggestions = text
                    .split('\n')
                    .slice(0, 3)
                    .map(line => line.trim().replace(/^-/, '').trim())
                this.sendSuggestions(suggestions)
                return Promise.resolve()
            },
        })

        let textConsumed = 0
        this.chat.chat(prompt, {
            onChange: text => {
                // TODO(dpc): The multiplexer can handle incremental text. Change chat to provide incremental text.
                text = text.slice(textConsumed)
                textConsumed += text.length
                void multiplexer.publish(text)
            },
            onComplete: () => {
                void multiplexer.notifyTurnComplete()
            },
            onError: (error, statusCode) => {
                console.error(error, statusCode)
            },
        })
    }

    private showTab(tab: string): void {
        void vscode.commands.executeCommand('cody.chat.focus')
        void this.webview?.postMessage({ type: 'showTab', tab })
    }

    /**
     * Send transcript to webview
     */
    private sendTranscript(): void {
        void this.webview?.postMessage({
            type: 'transcript',
            messages: this.transcript.toChat(),
            isMessageInProgress: this.isMessageInProgress,
        })
    }

    private sendSuggestions(suggestions: string[]): void {
        void this.webview?.postMessage({
            type: 'suggestions',
            suggestions,
        })
    }

    private async saveTranscriptToChatHistory(): Promise<void> {
        if (this.transcript.isEmpty) {
            return
        }
        this.chatHistory[this.currentChatID] = await this.transcript.toJSON()
        await this.saveChatHistory()
    }

    /**
     * Save chat history
     */
    private async saveChatHistory(): Promise<void> {
        const userHistory = {
            chat: this.chatHistory,
            input: this.inputHistory,
        }
        await this.localStorage.setChatHistory(userHistory)
    }

    /**
     * Save Login state to webview
     */
    public async sendLogin(isValid: boolean): Promise<void> {
        await vscode.commands.executeCommand('setContext', 'cody.activated', isValid)
        if (isValid) {
            this.sendEvent('auth', 'login')
        }
        void this.webview?.postMessage({ type: 'login', isValid })
    }

    /**
     * Loads chat history from local storage
     */
    private loadChatHistory(): void {
        const localHistory = this.localStorage.getChatHistory()
        if (localHistory) {
            this.chatHistory = localHistory?.chat
            this.inputHistory = localHistory.input
        }
    }

    /**
     * Sends chat history to webview
     */
    private sendChatHistory(): void {
        void this.webview?.postMessage({
            type: 'history',
            messages: {
                chat: this.chatHistory,
                input: this.inputHistory,
            },
        })
    }

    /**
     * Publish the current context status to the webview.
     */
    private publishContextStatus(): void {
        const send = (): void => {
            const editorContext = this.editor.getActiveTextEditor()
            void this.webview?.postMessage({
                type: 'contextStatus',
                contextStatus: {
                    mode: this.config.useContext,
                    connection: this.codebaseContext.checkEmbeddingsConnection(),
                    codebase: this.codebaseContext.getCodebase(),
                    filePath: editorContext ? vscode.workspace.asRelativePath(editorContext.filePath) : undefined,
                    supportsKeyword: true,
                },
            })
        }

        this.disposables.push(vscode.window.onDidChangeTextEditorSelection(() => send()))
        send()
    }

    /**
     * Publish the config to the webview.
     */
    private publishConfig(): void {
        const send = async (): Promise<void> => {
            // update codebase context on configuration change
            void this.updateCodebaseContext()
            // check if new configuration change is valid or not
            // log user out if config is invalid
            const isAuthed = await isValidLogin({
                serverEndpoint: this.config.serverEndpoint,
                accessToken: this.config.accessToken,
                customHeaders: this.config.customHeaders,
            })
            const configForWebview: ConfigurationSubsetForWebview = {
                debug: this.config.debug,
                serverEndpoint: this.config.serverEndpoint,
                hasAccessToken: isAuthed,
            }
            void vscode.commands.executeCommand('setContext', 'cody.activated', isAuthed)
            void this.webview?.postMessage({ type: 'config', config: configForWebview })
        }

        this.disposables.push(this.configurationChangeEvent.event(() => send()))
        send().catch(error => console.error(error))
    }

    /**
     * Log Events
     */
    public sendEvent(event: string, value: string): void {
        const isPrivateInstance = new URL(this.config.serverEndpoint).href !== DOTCOM_URL.href
        switch (event) {
            case 'feedback':
                // Only include context for dot com users with connected codebase
                logEvent(
                    `CodyVSCodeExtension:codyFeedback:${value}`,
                    null,
                    !isPrivateInstance && this.codebaseContext.getCodebase() ? this.transcript.toChat() : null
                )
                break
            case 'token':
                logEvent(`CodyVSCodeExtension:cody${value}AccessToken:clicked`)
                break
            case 'auth':
                logEvent(`CodyVSCodeExtension:${value}:clicked`)
                break
            // aditya combine this with above statemenet for auth or click
            case 'click':
                logEvent(`CodyVSCodeExtension:${value}:clicked`)
                break
        }
    }

    /**
     * Display error message in webview view as banner in chat view
     * It does not display error message as assistant response
     */
    public sendErrorToWebview(errorMsg: string): void {
        void this.webview?.postMessage({ type: 'errors', errors: errorMsg })
    }

    /**
     * Set webview view
     */
    public setWebviewView(view: View): void {
        void vscode.commands.executeCommand('cody.chat.focus')
        void this.webview?.postMessage({
            type: 'view',
            messages: view,
        })
    }

    /**
     * create webview resources
     */
    public async resolveWebviewView(
        webviewView: vscode.WebviewView,
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        _context: vscode.WebviewViewResolveContext<unknown>,
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        _token: vscode.CancellationToken
    ): Promise<void> {
        this.webview = webviewView.webview

        const extensionPath = vscode.Uri.file(this.extensionPath)
        const webviewPath = vscode.Uri.joinPath(extensionPath, 'dist')

        webviewView.webview.options = {
            enableScripts: true,
            localResourceRoots: [webviewPath],
        }

        // Create Webview using client/cody/index.html
        const root = vscode.Uri.joinPath(webviewPath, 'index.html')
        const bytes = await vscode.workspace.fs.readFile(root)
        const decoded = new TextDecoder('utf-8').decode(bytes)
        const resources = webviewView.webview.asWebviewUri(webviewPath)

        // Set HTML for webview
        // This replace variables from the client/cody/dist/index.html with webview info
        // 1. Update URIs to load styles and scripts into webview (eg. path that starts with ./)
        // 2. Update URIs for content security policy to only allow specific scripts to be run
        webviewView.webview.html = decoded
            .replaceAll('./', `${resources.toString()}/`)
            .replaceAll('{cspSource}', webviewView.webview.cspSource)

        // Register webview
        this.disposables.push(webviewView.webview.onDidReceiveMessage(message => this.onDidReceiveMessage(message)))
    }

    /**
     * Open external links
     */
    private async openExternalLinks(uri: string): Promise<void> {
        try {
            await vscode.env.openExternal(vscode.Uri.parse(uri))
        } catch (error) {
            throw new Error(`Failed to open file: ${error}`)
        }
    }

    public transcriptForTesting(testing: TestSupport): ChatMessage[] {
        if (!testing) {
            console.error('used ForTesting method without test support object')
            return []
        }
        return this.transcript.toChat()
    }

    public dispose(): void {
        for (const disposable of this.disposables) {
            disposable.dispose()
        }
        this.disposables = []
    }
}

function filePathContains(container: string, contained: string): boolean {
    let trimmedContained = contained
    if (trimmedContained.endsWith(path.sep)) {
        trimmedContained = trimmedContained.slice(0, -path.sep.length)
    }
    if (trimmedContained.startsWith(path.sep)) {
        trimmedContained = trimmedContained.slice(path.sep.length)
    }
    return (
        container.includes(path.sep + trimmedContained + path.sep) || // mid-level directory
        container.endsWith(path.sep + trimmedContained) // child
    )
}

async function filesExist(filePaths: string[]): Promise<{ [filePath: string]: boolean }> {
    const searchPath = `{${filePaths.join(',')}}`
    const realFiles = await vscode.workspace.findFiles(searchPath, null, filePaths.length * 5)
    const ret: { [filePath: string]: boolean } = {}
    for (const filePath of filePaths) {
        let pathExists = false
        for (const realFile of realFiles) {
            if (filePathContains(realFile.fsPath, filePath)) {
                pathExists = true
                break
            }
        }
        ret[filePath] = pathExists
    }
    return ret
}

// Converts a git clone URL to the codebase name that includes the slash-separated code host, owner, and repository name
// This should captures:
// - "github:sourcegraph/sourcegraph" a common SSH host alias
// - "https://github.com/sourcegraph/deploy-sourcegraph-k8s.git"
// - "git@github.com:sourcegraph/sourcegraph.git"
export function convertGitCloneURLToCodebaseName(cloneURL: string): string | null {
    if (!cloneURL) {
        console.error(`Unable to determine the git clone URL for this workspace.\ngit output: ${cloneURL}`)
        return null
    }
    try {
        const uri = new URL(cloneURL.replace('git@', ''))
        // Handle common Git SSH URL format
        const match = cloneURL.match(/git@([^:]+):([\w-]+)\/([\w-]+)(\.git)?/)
        if (cloneURL.startsWith('git@') && match) {
            const host = match[1]
            const owner = match[2]
            const repo = match[3]
            return `${host}/${owner}/${repo}`
        }
        // Handle GitHub URLs
        if (uri.protocol.startsWith('github') || uri.href.startsWith('github')) {
            return `github.com/${uri.pathname.replace('.git', '')}`
        }
        // Handle GitLab URLs
        if (uri.protocol.startsWith('gitlab') || uri.href.startsWith('gitlab')) {
            return `gitlab.com/${uri.pathname.replace('.git', '')}`
        }
        // Handle HTTPS URLs
        if (uri.protocol.startsWith('http') && uri.hostname && uri.pathname) {
            return `${uri.hostname}${uri.pathname.replace('.git', '')}`
        }
        // Generic URL
        if (uri.hostname && uri.pathname) {
            return `${uri.hostname}${uri.pathname.replace('.git', '')}`
        }
        return null
    } catch (error) {
        console.error(`Cody could not extract repo name from clone URL ${cloneURL}:`, error)
        return null
    }
}

async function getCodebaseContext(config: Config, rgPath: string, editor: Editor): Promise<CodebaseContext | null> {
    const client = new SourcegraphGraphQLAPIClient(config)
    const workspaceRoot = editor.getWorkspaceRootPath()
    if (!workspaceRoot) {
        return null
    }
    const gitCommand = spawnSync('git', ['remote', 'get-url', 'origin'], { cwd: workspaceRoot })
    const gitOutput = gitCommand.stdout.toString().trim()
    // Get codebase from config or fallback to getting repository name from git clone URL
    const codebase = config.codebase || convertGitCloneURLToCodebaseName(gitOutput)
    if (!codebase) {
        return null
    }
    // Check if repo is embedded in endpoint
    const repoId = await client.getRepoIdIfEmbeddingExists(codebase)
    if (isError(repoId)) {
        const infoMessage = `Cody could not find embeddings for '${codebase}' on your Sourcegraph instance.\n`
        console.info(infoMessage)
        return null
    }

    const embeddingsSearch = repoId && !isError(repoId) ? new SourcegraphEmbeddingsSearchClient(client, repoId) : null
    return new CodebaseContext(config, codebase, embeddingsSearch, new LocalKeywordContextFetcher(rgPath, editor))
}
