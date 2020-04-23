import { Remote } from '@sourcegraph/comlink'
import { Range, Selection } from '@sourcegraph/extension-api-classes'
import * as clientType from '@sourcegraph/extension-api-types'
import { BehaviorSubject } from 'rxjs'
import * as sourcegraph from 'sourcegraph'
import { ClientCodeEditorAPI } from '../../client/api/codeEditor'
import { CodeEditorData, EditorId } from '../../client/services/editorService'
import { createDecorationType } from './decorations'
import { ExtDocuments } from './documents'

const DEFAULT_DECORATION_TYPE = createDecorationType()

/**
 * Returns true if all of the objects properties are empty null, undefined, empty strings or objects that are also empty.
 */
const isEmptyObjectDeep = (value: any): boolean =>
    Array.isArray(value)
        ? value.every(isEmptyObjectDeep)
        : typeof value === 'object' && value !== null
        ? Object.values(value).every(isEmptyObjectDeep)
        : !value

const isDecorationEmpty = ({ range, isWholeLine, ...contents }: clientType.TextDocumentDecoration): boolean =>
    isEmptyObjectDeep(contents)

/** @internal */
export class ExtCodeEditor implements sourcegraph.CodeEditor {
    /** The URI of this editor's document. */
    private resource: string

    constructor(
        data: CodeEditorData & EditorId,
        private proxy: Remote<ClientCodeEditorAPI>,
        private documents: ExtDocuments
    ) {
        this.resource = data.resource
        this.update(data)
    }

    public readonly selectionsChanges = new BehaviorSubject<sourcegraph.Selection[]>([])

    public readonly type = 'CodeEditor'

    public get document(): sourcegraph.TextDocument {
        return this.documents.get(this.resource)
    }

    public get selection(): sourcegraph.Selection | null {
        return this.selectionsChanges.value.length > 0 ? this.selectionsChanges.value[0] : null
    }

    public get selections(): sourcegraph.Selection[] {
        return this.selectionsChanges.value
    }

    public setDecorations(
        decorationType: sourcegraph.TextDocumentDecorationType | null,
        decorations: sourcegraph.TextDocumentDecoration[]
    ): void {
        // Backcompat: extensions developed against an older version of the API
        // may not supply a decorationType
        decorationType = decorationType || DEFAULT_DECORATION_TYPE
        // eslint-disable-next-line @typescript-eslint/no-floating-promises
        this.proxy.$setDecorations(
            this.resource,
            decorationType.key,
            decorations.map(fromTextDocumentDecoration).filter(decoration => !isDecorationEmpty(decoration))
        )
    }

    public update(data: Pick<CodeEditorData, 'selections'>): void {
        this.selectionsChanges.next(data.selections.map(s => Selection.fromPlain(s)))
    }

    public toJSON(): any {
        return { type: this.type, document: this.document }
    }
}

function fromTextDocumentDecoration(decoration: sourcegraph.TextDocumentDecoration): clientType.TextDocumentDecoration {
    return {
        ...decoration,
        range: (decoration.range as Range).toJSON(),
    }
}
