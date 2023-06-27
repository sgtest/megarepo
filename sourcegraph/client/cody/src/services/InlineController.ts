import * as vscode from 'vscode'

import { ActiveTextEditorSelection } from '@sourcegraph/cody-shared/src/editor'
import { SURROUNDING_LINES } from '@sourcegraph/cody-shared/src/prompt/constants'

import { logEvent } from '../event-logger'
import { CodyTaskState } from '../non-stop/utils'

import { CodeLensProvider } from './CodeLensProvider'
import { editDocByUri, getIconPath, updateRangeOnDocChange } from './InlineAssist'

const initPost = new vscode.Position(0, 0)
const initRange = new vscode.Range(initPost, initPost)

export class InlineController {
    // Controller init
    private readonly id = 'cody-inline-chat'
    private readonly label = 'Cody: Inline Chat'
    private readonly threadLabel =
        '[TIPS] New Inline Chat: `ctrl + shift + c` | Submit: `cmd + enter` | Hide: `shift + esc`'
    private options = {
        prompt: 'Cody Inline Chat - Ask Cody a question or request inline fix with `/fix` or `/touch`.',
        placeHolder:
            'Examples: "How can I improve this?", "/fix convert tabs to spaces", "/touch Create 5 different versions of this function". "What does this regex do?"',
    }
    private readonly codyIcon: vscode.Uri = getIconPath('cody', this.extensionPath)
    private readonly userIcon: vscode.Uri = getIconPath('user', this.extensionPath)
    private _disposables: vscode.Disposable[] = []
    // Constroller State
    private commentController: vscode.CommentController | null = null
    public thread: vscode.CommentThread | null = null // a thread is a comment
    private threads = new Map<string, vscode.CommentThread>()
    private currentTaskId = ''
    // Workspace State
    private workspacePath = vscode.workspace.workspaceFolders?.[0].uri
    public selection: ActiveTextEditorSelection | null = null
    public selectionRange = initRange
    // Inline Tasks States
    public isInProgress = false
    private codeLenses: Map<string, CodeLensProvider> = new Map()

    constructor(private extensionPath: string) {
        this.commentController = this.init()
        this._disposables.push(this.commentController)
        // Toggle Inline Chat on Config Change
        vscode.workspace.onDidChangeConfiguration(e => {
            const config = vscode.workspace.getConfiguration('cody')
            if (e.affectsConfiguration('cody')) {
                // Inline Chat
                const enableInlineChat = config.get('inlineChat.enabled') as boolean
                if (enableInlineChat) {
                    this.commentController = this.init()
                    return
                }
                this.commentController?.dispose()
                this.commentController = null
                this.dispose()
            }
        })
        // Track last selection range in valid doc before an action is called
        vscode.window.onDidChangeTextEditorSelection(e => {
            if (
                e.textEditor.document.uri.scheme !== 'file' ||
                e.textEditor.document.uri.fsPath !== this.thread?.uri.fsPath
            ) {
                return
            }
            const selection = e.selections[0]
            if (selection && !this.isInProgress && this.selectionRange.end.line - 2 !== selection.start.line) {
                const range = new vscode.Range(
                    new vscode.Position(Math.max(0, selection.start.line - 1), 0),
                    new vscode.Position(Math.max(0, selection.end.line + 2), 0)
                )
                this.selectionRange = range
            }
        })
        // Track and update line diff when a task for the current selected range is being processed (this.isInProgress)
        // This makes sure the comment range and highlights are also updated correctly
        vscode.workspace.onDidChangeTextDocument(e => {
            // don't track
            if (
                !this.isInProgress ||
                !this.selectionRange ||
                e.document.uri.scheme !== 'file' ||
                e.document.uri.fsPath !== this.thread?.uri.fsPath
            ) {
                return
            }
            for (const change of e.contentChanges) {
                this.selectionRange = updateRangeOnDocChange(this.selectionRange, change.range, change.text)
            }
        })
        // Remove all the threads from current file on file close
        vscode.workspace.onDidCloseTextDocument(doc => {
            if (doc.uri.scheme !== 'file') {
                return
            }
            const threadsInDoc = [...this.threads.values()].filter(thread => thread.uri.fsPath === doc.uri.fsPath)
            for (const thread of threadsInDoc) {
                this.delete(thread)
            }
        })
        this._disposables.push(
            vscode.commands.registerCommand('cody.inline.decorations.remove', id => this.removeLens(id)),
            vscode.commands.registerCommand('cody.inline.fix.undo', id => this.undo(id))
        )
    }
    /**
     * Create comment controller and set options
     */
    public init(): vscode.CommentController {
        this.commentController?.dispose()
        const commentController = vscode.comments.createCommentController(this.id, this.label)
        commentController.options = this.options
        commentController.commentingRangeProvider = {
            provideCommentingRanges: (document: vscode.TextDocument) => {
                const lineCount = document.lineCount
                return [new vscode.Range(0, 0, lineCount - 1, 0)]
            },
        }
        return commentController
    }
    /**
     * Getter to return comment controller
     */
    public get(): vscode.CommentController | null {
        return this.commentController
    }
    /**
     * Create a new thread (the first comment of a thread)
     */
    public create(humanInput: string): vscode.CommentReply | null {
        if (!this.commentController) {
            return null
        }
        const editor = vscode.window.activeTextEditor
        if (!editor || !humanInput || editor.document.uri.scheme !== 'file') {
            return null
        }
        this.thread = this.commentController.createCommentThread(editor?.document.uri, editor.selection, [])
        this.thread.collapsibleState = vscode.CommentThreadCollapsibleState.Collapsed
        const threads = {
            text: humanInput,
            thread: this.thread,
        }
        return threads
    }
    /**
     * List response from Human as comment
     */
    public async chat(threads: vscode.CommentReply, isFixMode: boolean = false): Promise<void> {
        this.isInProgress = true
        const humanInput = threads.text
        const thread = threads.thread
        // disable reply until the task is completed
        thread.canReply = false
        thread.label = this.threadLabel
        thread.collapsibleState = vscode.CommentThreadCollapsibleState.Collapsed
        const comment = new Comment(humanInput, 'Me', this.userIcon, isFixMode, thread, 'loading')
        thread.comments = [...thread.comments, comment]
        await this.runFixMode(isFixMode, comment, thread)
        this.thread = thread
        this.selection = await this.makeSelection(isFixMode)
        const firstCommentId = thread.comments[0].label
        if (firstCommentId) {
            this.threads.set(firstCommentId, thread)
        }
        void vscode.commands.executeCommand('setContext', 'cody.replied', false)
    }
    /**
     * List response from Cody as comment
     */
    public reply(text: string, error = false): void {
        if (!this.thread || this.thread.state) {
            return
        }
        const replyText = text
        const comment = new Comment(replyText, 'Cody', this.codyIcon, false, this.thread, undefined)
        this.thread.comments = [...this.thread.comments, comment]
        this.thread.canReply = !error
        this.thread.state = error ? 1 : 0
        const firstCommentId = this.thread.comments[0].label
        if (firstCommentId) {
            this.threads.set(firstCommentId, this.thread)
        }
        void vscode.commands.executeCommand('setContext', 'cody.replied', true)
    }

    private undo(id: string): void {
        void this.codeLenses.get(id)?.undo(id)
        this.codeLenses.delete(id)
    }
    /**
     * Remove a comment thread / conversation
     */
    public delete(thread: vscode.CommentThread): void {
        if (!thread) {
            return
        }
        const comments = thread?.comments as Comment[]
        comments.map(comment => {
            this.codeLenses.get(comment.id)?.remove()
        })
        thread.dispose()
        this.reset()
    }
    /**
     * Reset class
     */
    public reset(): void {
        this.selectionRange = initRange
        this.thread = null
    }

    public async error(): Promise<void> {
        this.reply('Request failed. Please close this and try again.', true)
        if (this.currentTaskId) {
            await this.stopFixMode(true)
        }
    }
    /**
     * Create code lense and initiate decorators for fix mode
     */
    private async runFixMode(isFixMode: boolean, comment: Comment, thread: vscode.CommentThread): Promise<void> {
        if (!isFixMode) {
            return
        }
        const lens = await this.makeCodeLenses(comment.id, this.extensionPath, thread)
        lens.updateState(CodyTaskState.asking, thread.range)
        this.codeLenses.set(comment.id, lens)
        this.currentTaskId = comment.id
        void vscode.commands.executeCommand('workbench.action.collapseAllComments')
    }
    /**
     * Reset the selection range once replacement started by fixup has been completed
     * Then inform the dependents (eg. Code Lenses and Decorators) about the new range
     * so that they could update accordingly
     */
    private async stopFixMode(error = false, newRange?: vscode.Range): Promise<void> {
        this.isInProgress = false
        if (!this.currentTaskId) {
            return
        }
        const range = newRange || this.selectionRange
        const status = error ? CodyTaskState.error : CodyTaskState.fixed
        const lens = this.codeLenses.get(this.currentTaskId)
        lens?.updateState(status, range)
        if (this.thread) {
            this.thread.range = range
            this.thread.state = error ? 1 : 0
        }
        this.currentTaskId = ''
        logEvent('CodyVSCodeExtension:inline-assist:stopFixup')
        if (!error) {
            await vscode.commands.executeCommand('workbench.action.collapseAllComments')
        }
    }
    /**
     * Get current selected lines from the comment thread.
     * Add an extra line to the end line to prevent empty selection on single line selection
     */
    public async makeSelection(isFixMode: boolean): Promise<ActiveTextEditorSelection | null> {
        if (!this.thread) {
            return null
        }
        const activeDocument = await vscode.workspace.openTextDocument(this.thread.uri)
        const lineLength = activeDocument.lineAt(this.thread.range.end.line).text.length
        const startPost = new vscode.Position(this.thread.range.start.line, 0)
        const endPostFix = new vscode.Position(this.thread.range.end.line, lineLength)
        const endPostAsk = new vscode.Position(this.thread.range.end.line + 1, 0)
        const selectionRange = new vscode.Range(startPost, isFixMode ? endPostFix : endPostAsk)
        const precedingText = activeDocument.getText(
            new vscode.Range(
                new vscode.Position(Math.max(0, this.thread.range.start.line - SURROUNDING_LINES), 0),
                this.thread.range.start
            )
        )
        const followingText = activeDocument.getText(
            new vscode.Range(
                this.thread.range.end,
                new vscode.Position(this.thread.range.end.line + 1 + SURROUNDING_LINES, 0)
            )
        )
        // Add space when selectedText is empty --empty selectedText could cause delayed response
        const selection = {
            fileName: vscode.workspace.asRelativePath(this.thread.uri.fsPath),
            selectedText: activeDocument.getText(selectionRange) || ' ',
            precedingText,
            followingText,
        }
        this.selectionRange = selectionRange
        this.selection = selection
        return selection
    }
    /**
     * When a comment thread is open, the Editor will be switched to the comment input editor.
     * Get the current editor using the comment thread uri instead
     */
    public async makeCodeLenses(
        taskID: string,
        extPath: string,
        thread: vscode.CommentThread
    ): Promise<CodeLensProvider> {
        const lens = new CodeLensProvider(taskID, extPath, thread)
        const activeDocument = await vscode.workspace.openTextDocument(thread.uri)
        await lens.provideCodeLenses(activeDocument, new vscode.CancellationTokenSource().token)
        vscode.languages.registerCodeLensProvider('*', lens)
        return lens
    }

    public removeLens(id: string): void {
        this.codeLenses.get(id)?.remove()
        this.codeLenses.delete(id)
    }
    /**
     * Do replacement in document
     */
    public async replace(fileName: string, replacement: string, original: string): Promise<void> {
        const diff = original.trim() !== replacement.trim()
        if (!this.workspacePath || !replacement.trim() || !diff) {
            await this.stopFixMode(true)
            return
        }
        // Stop tracking for file changes to perfotm replacement
        this.isInProgress = false
        const chatSelection = this.getSelectionRange()
        const documentUri = vscode.Uri.joinPath(this.workspacePath, fileName)
        const range = new vscode.Selection(chatSelection.start, new vscode.Position(chatSelection.end.line + 1, 0))
        const newRange = await editDocByUri(documentUri, { start: range.start.line, end: range.end.line }, replacement)

        const lens = this.codeLenses.get(this.currentTaskId)
        lens?.storeContext(this.currentTaskId, documentUri, original, replacement)

        await this.stopFixMode(false, newRange)
        logEvent('CodyVSCodeExtension:inline-assist:replaced')
    }
    /**
     * Return latest selection
     */
    public getSelection(): ActiveTextEditorSelection | null {
        return this.selection
    }
    /**
     * Return latest selection range
     */
    public getSelectionRange(): vscode.Range {
        return this.selectionRange
    }
    /**
     * Dispose the disposables
     */
    public dispose(): void {
        for (const disposable of this._disposables) {
            disposable.dispose()
        }
        this._disposables = []
    }
}

export class Comment implements vscode.Comment {
    public id: string
    public label: string | undefined
    public body: string | vscode.MarkdownString
    public mode = vscode.CommentMode.Preview
    public author: vscode.CommentAuthorInformation
    constructor(
        public input: string,
        public name: string,
        public iconPath: vscode.Uri,
        public isTask: boolean,
        public parent?: vscode.CommentThread,
        public contextValue?: string
    ) {
        const timestamp = new Date(Date.now())
        this.id = timestamp.getTime().toString()
        this.body = this.markdown(input)
        this.author = { name, iconPath }
        this.label = '#' + this.id
    }

    /**
     * Turns string into Markdown string
     */
    private markdown(text: string): vscode.MarkdownString {
        const markdownText = new vscode.MarkdownString(text)
        markdownText.isTrusted = true
        markdownText.supportHtml = true
        return markdownText
    }
}
