import {
    ActiveTextEditor,
    ActiveTextEditorSelection,
    ActiveTextEditorVisibleContent,
    Editor,
} from '@sourcegraph/cody-shared/src/editor'

export interface EditorStore {
    filePath: string
    repoName: string
    revision: string
    content: string
}

export class FileContentEditor implements Editor {
    private editor: EditorStore
    constructor(editor: EditorStore) {
        this.editor = editor
    }

    public getWorkspaceRootPath(): string | null {
        return null
    }

    public getActiveTextEditor(): ActiveTextEditor | null {
        return this.editor
    }

    public getActiveTextEditorSelection(): ActiveTextEditorSelection | null {
        return null
    }

    public getActiveTextEditorSelectionOrEntireFile(): ActiveTextEditorSelection | null {
        return {
            fileName: this.editor.filePath,
            repoName: this.editor.repoName,
            revision: this.editor.revision,
            precedingText: '',
            selectedText: this.editor.content,
            followingText: '',
        }
    }

    public getActiveTextEditorVisibleContent(): ActiveTextEditorVisibleContent | null {
        return {
            ...this.editor,
            fileName: this.editor.filePath,
        }
    }

    public replaceSelection(_fileName: string, _selectedText: string, _replacement: string): Promise<void> {
        // Not implemented.
        return Promise.resolve()
    }

    public showQuickPick(labels: string[]): Promise<string | undefined> {
        // Not implemented.
        return Promise.resolve(window.prompt(`Choose between: ${labels.join(', ')}`, labels[0]) || undefined)
    }

    public async showWarningMessage(message: string): Promise<void> {
        // Not implemented.
        // eslint-disable-next-line no-console
        console.warn(message)
        return Promise.resolve()
    }

    public showInputBox(): Promise<string | undefined> {
        // Not implemented.
        return Promise.resolve(window.prompt('Enter your answer: ') || undefined)
    }

    public didReceiveFixupText(id: string, text: string, state: 'streaming' | 'complete'): Promise<void> {
        // Not implemented.
        return Promise.resolve(undefined)
    }
}
