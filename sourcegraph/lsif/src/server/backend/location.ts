import * as lsp from 'vscode-languageserver-protocol'
import * as pgModels from '../../shared/models/pg'

/** A location with the identifier of the dump that contains it. */
export interface InternalLocation {
    dumpId: pgModels.DumpId
    path: string
    range: lsp.Range
}

/** A location with the dump that contains it. */
export interface ResolvedInternalLocation {
    dump: pgModels.LsifDump
    path: string
    range: lsp.Range
}

/** A duplicate-free list of locations ordered by time of insertion. */
export class OrderedLocationSet {
    private seen = new Set<string>()
    private order: InternalLocation[] = []

    /** The deduplicated locations in insertion order. */
    public get locations(): InternalLocation[] {
        return this.order
    }

    /** Insert a location into the set if it hasn't been seen before. */
    public push(location: InternalLocation): void {
        const key = makeKey(location)
        if (this.seen.has(key)) {
            return
        }

        this.seen.add(key)
        this.order.push(location)
    }
}

/** Makes a unique string representation of this location. */
function makeKey(location: InternalLocation): string {
    return [
        location.dumpId,
        location.path,
        location.range.start.line,
        location.range.start.character,
        location.range.end.line,
        location.range.end.character,
    ].join(':')
}
