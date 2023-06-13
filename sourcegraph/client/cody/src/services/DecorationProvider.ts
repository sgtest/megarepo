import * as vscode from 'vscode'

import { CodyTaskState } from '../non-stop/utils'

import { getIconPath, getSingleLineRange } from './InlineAssist'

const initDecorationType = vscode.window.createTextEditorDecorationType({})

export class DecorationProvider {
    private iconPath: vscode.Uri
    private fileUri: vscode.Uri | null = null
    private status = CodyTaskState.idle

    private static decorates: DecorationProvider
    private decorations: vscode.DecorationOptions[] = []
    private decorationsForIcon: vscode.DecorationOptions[] = []

    private decorationTypePending = this.makeDecorationType('pending')
    private decorationTypeDiff = this.makeDecorationType('diff')
    private decorationTypeError = this.makeDecorationType('error')
    private decorationTypeIcon = initDecorationType

    private _disposables: vscode.Disposable[] = []
    private _onDidChange: vscode.EventEmitter<void> = new vscode.EventEmitter<void>()
    public readonly onDidChange: vscode.Event<void> = this._onDidChange.event

    constructor(public id = '', private extPath = '') {
        // set up icon and register decoration types
        this.iconPath = getIconPath('cody', this.extPath)
        this.decorationTypeIcon = this.makeDecorationType('icon')
        this._disposables.push(
            this.decorationTypeIcon,
            this.decorationTypeDiff,
            this.decorationTypePending,
            this.decorationTypeError
        )
    }
    /**
     * Getter
     */
    public static get instance(): DecorationProvider {
        return (this.decorates ??= new this())
    }
    /**
     * Highlights line where the codes updated by Cody are located.
     */
    public async decorate(range: vscode.Range): Promise<void> {
        if (!this.fileUri || !range) {
            return
        }
        const currentFile = await vscode.workspace.openTextDocument(this.fileUri)
        if (!currentFile) {
            return
        }
        await vscode.window.showTextDocument(this.fileUri)
        const rangeStartLine = getSingleLineRange(range.start.line)
        if (this.status === CodyTaskState.error) {
            this.decorationTypePending.dispose()
            this.decorations.push({ range, hoverMessage: 'Failed Cody Assist #' + this.id })
            this.decorationsForIcon.push({ range: rangeStartLine })
            vscode.window.activeTextEditor?.setDecorations(this.decorationTypeError, this.decorations)
            await vscode.window.showTextDocument(this.fileUri, { selection: rangeStartLine })
            return
        }
        if (this.status === CodyTaskState.fixed) {
            this.decorationTypePending.dispose()
            this.decorations.push({ range, hoverMessage: 'Cody Assist #' + this.id })
            this.decorationsForIcon.push({ range: rangeStartLine })
            vscode.window.activeTextEditor?.setDecorations(this.decorationTypeIcon, this.decorationsForIcon)
            vscode.window.activeTextEditor?.setDecorations(this.decorationTypeDiff, this.decorations)
            await vscode.window.showTextDocument(this.fileUri, { selection: rangeStartLine })
            return
        }
        vscode.window.activeTextEditor?.setDecorations(this.decorationTypePending, [
            { range, hoverMessage: 'Do not make changes to the highlighted code while Cody is working on it.' },
        ])
    }
    /**
     * Clear all decorations
     */
    public async clear(): Promise<void> {
        if (!this.fileUri) {
            return
        }
        await vscode.workspace.openTextDocument(this.fileUri)
        this.decorationTypePending.dispose()
        this.decorationTypeIcon.dispose()
        this.decorationTypeDiff.dispose()
        this.decorationTypeError.dispose()
    }
    public setFileUri(uri: vscode.Uri): void {
        this.fileUri = uri
    }
    /**
     * Define Current States
     */
    public setState(status: CodyTaskState, newRange: vscode.Range): void {
        this.status = status
        vscode.window.activeTextEditor?.setDecorations(this.decorationTypePending, [newRange])
        this._onDidChange.fire()
    }
    /**
     * Remove everything created for task
     */
    public remove(): void {
        this.dispose()
    }
    /**
     * Define styles
     */
    private makeDecorationType(type?: string): vscode.TextEditorDecorationType {
        if (type === 'icon') {
            return vscode.window.createTextEditorDecorationType({
                gutterIconPath: this.iconPath,
                gutterIconSize: 'contain',
            })
        }
        if (type === 'error') {
            return errorDecorationType
        }
        return vscode.window.createTextEditorDecorationType({
            isWholeLine: true,
            borderWidth: '1px',
            borderStyle: 'solid',
            overviewRulerColor: type === 'pending' ? 'rgba(161, 18, 255, 0.33)' : 'rgb(0, 203, 236, 0.22)',
            backgroundColor: type === 'pending' ? 'rgb(0, 203, 236, 0.1)' : 'rgba(161, 18, 255, 0.1)',
            overviewRulerLane: vscode.OverviewRulerLane.Right,
            light: {
                borderColor: 'rgba(161, 18, 255, 0.33)',
            },
            dark: {
                borderColor: 'rgba(161, 18, 255, 0.33)',
            },
        })
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

const errorDecorationType = vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    overviewRulerColor: 'rgba(255, 38, 86, 0.3)',
    backgroundColor: 'rgba(255, 38, 86, 0.1)',
})
