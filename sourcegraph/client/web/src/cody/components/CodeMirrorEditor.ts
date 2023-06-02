import type { EditorView } from '@codemirror/view'

import {
    ActiveTextEditor,
    ActiveTextEditorSelection,
    ActiveTextEditorVisibleContent,
    Editor,
} from '@sourcegraph/cody-shared/src/editor'

export interface EditorStore {
    filename: string
    repo: string
    revision: string
    content: string
    view: EditorView
}

export class CodeMirrorEditor implements Editor {
    private editor?: EditorStore | null
    constructor(editor?: EditorStore | null) {
        this.editor = editor
    }

    public get repoName(): string | undefined {
        return this.editor?.repo
    }

    public get revision(): string | undefined {
        return this.editor?.revision
    }

    public getWorkspaceRootPath(): string | null {
        return null
    }

    public getActiveTextEditor(): ActiveTextEditor | null {
        const editor = this.editor
        if (editor) {
            return {
                content: editor.content,
                filePath: editor.filename,
                repoName: this.repoName,
                revision: this.revision,
            }
        }
        return null
    }

    public getActiveTextEditorSelection(): ActiveTextEditorSelection | null {
        const editor = this.editor

        if (!editor || editor.view.state.selection.main.empty) {
            return null
        }

        const selection = editor.view?.state.selection.main
        const { head, anchor } = selection

        if (head !== anchor) {
            const precedingText = editor.view?.state.sliceDoc(undefined, selection.from)
            const selectedText = editor.view?.state.sliceDoc(selection.from, selection.to)
            const followingText = editor.view?.state.sliceDoc(selection.to, undefined)

            return {
                fileName: editor.filename,
                repoName: this.repoName,
                revision: this.revision,
                precedingText,
                selectedText,
                followingText,
            }
        }

        return null
    }

    public getActiveTextEditorSelectionOrEntireFile(): ActiveTextEditorSelection | null {
        if (this.editor) {
            const selection = this.getActiveTextEditorSelection()
            if (selection) {
                return selection
            }

            return {
                fileName: this.editor.filename,
                repoName: this.repoName,
                revision: this.revision,
                precedingText: '',
                selectedText: this.editor.content,
                followingText: '',
            }
        }

        return null
    }

    public getActiveTextEditorVisibleContent(): ActiveTextEditorVisibleContent | null {
        const editor = this.editor
        if (editor) {
            const { from, to } = editor.view.viewport

            const content = editor.view?.state.sliceDoc(from, to)
            return {
                fileName: editor.filename,
                repoName: this.repoName,
                revision: this.revision,
                content,
            }
        }

        return null
    }

    public replaceSelection(_fileName: string, _selectedText: string, _replacement: string): Promise<void> {
        return Promise.resolve()
    }

    public showQuickPick(labels: string[]): Promise<string | undefined> {
        // TODO: Use a proper UI element
        return Promise.resolve(window.prompt(`Choose between: ${labels.join(', ')}`, labels[0]) || undefined)
    }

    public async showWarningMessage(message: string): Promise<void> {
        // TODO: Use a proper UI element
        // eslint-disable-next-line no-console
        console.warn(message)
        return Promise.resolve()
    }

    public showInputBox(): Promise<string | undefined> {
        // TODO: Use a proper UI element
        return Promise.resolve(window.prompt('Enter your answer: ') || undefined)
    }

    // TODO: When non-stop fixup is decoupled from chat and no longer a recipe,
    // remove this entrypoint.
    public didReceiveFixupText(id: string, text: string, state: 'streaming' | 'complete'): Promise<void> {
        // Not implemented.
        return Promise.resolve(undefined)
    }
}
