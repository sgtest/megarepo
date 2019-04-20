import { ProxyValue, proxyMarker } from '@sourcegraph/comlink'
import * as clientType from '@sourcegraph/extension-api-types'
import { Subject } from 'rxjs'
import * as sourcegraph from 'sourcegraph'

/** @internal */
export interface ExtRootsAPI extends ProxyValue {
    $acceptRoots(roots: readonly clientType.WorkspaceRoot[]): void
}

/** @internal */
export class ExtRoots implements ExtRootsAPI, ProxyValue {
    public readonly [proxyMarker] = true

    private roots: ReadonlyArray<sourcegraph.WorkspaceRoot> = []

    /**
     * Returns all workspace roots.
     *
     * @internal
     */
    public getAll(): ReadonlyArray<sourcegraph.WorkspaceRoot> {
        return this.roots
    }

    public readonly changes = new Subject<void>()

    public $acceptRoots(roots: clientType.WorkspaceRoot[]): void {
        this.roots = Object.freeze(roots.map(plain => ({ ...plain, uri: new URL(plain.uri) })))
        this.changes.next()
    }
}
